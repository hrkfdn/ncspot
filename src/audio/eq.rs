use std::sync::{Arc, RwLock};

pub const EQ_NUM_BANDS: usize = 10;
pub const EQ_MIN_GAIN_DB: f32 = -12.0;
pub const EQ_MAX_GAIN_DB: f32 = 12.0;
pub const EQ_SAMPLE_RATE: f32 = 44100.0;

pub const EQ_FREQUENCIES_HZ: [f32; EQ_NUM_BANDS] = [
    60.0, 170.0, 310.0, 600.0, 1000.0, 3000.0, 6000.0, 9000.0, 12000.0, 14000.0,
];

pub const EQ_BAND_NAMES: [&str; EQ_NUM_BANDS] = [
    "sub",
    "bass",
    "low_mid",
    "mid",
    "high_mid",
    "presence",
    "brilliance",
    "air",
    "upper",
    "super",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqState {
    pub enabled: bool,
    pub bands: [f32; EQ_NUM_BANDS],
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            enabled: false,
            bands: [0.0; EQ_NUM_BANDS],
        }
    }
}

impl EqState {
    pub fn clamp_gain(gain_db: f32) -> f32 {
        gain_db.clamp(EQ_MIN_GAIN_DB, EQ_MAX_GAIN_DB)
    }

    pub fn set_band(&mut self, index: usize, gain_db: f32) {
        if index < EQ_NUM_BANDS {
            self.bands[index] = Self::clamp_gain(gain_db);
        }
    }

    pub fn adjust_band(&mut self, index: usize, delta_db: f32) {
        if index < EQ_NUM_BANDS {
            self.bands[index] = Self::clamp_gain(self.bands[index] + delta_db);
        }
    }

    pub fn reset(&mut self) {
        self.bands = [0.0; EQ_NUM_BANDS];
    }

    pub fn resolve_band(name_or_index: &str) -> Option<usize> {
        if let Ok(index) = name_or_index.parse::<usize>() {
            return (index < EQ_NUM_BANDS).then_some(index);
        }
        EQ_BAND_NAMES.iter().position(|name| *name == name_or_index)
    }

    pub fn apply_preset(&mut self, name: &str) -> bool {
        if let Some(bands) = EQ_PRESETS
            .iter()
            .find(|(preset_name, _)| *preset_name == name)
            .map(|(_, bands)| *bands)
        {
            self.bands = bands;
            true
        } else {
            false
        }
    }
}

pub const EQ_PRESETS: &[(&str, [f32; EQ_NUM_BANDS])] = &[
    ("flat", [0.0; EQ_NUM_BANDS]),
    (
        "bass_boost",
        [4.0, 5.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    ),
    (
        "treble_boost",
        [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 5.0, 4.0, 3.0],
    ),
    (
        "vocal",
        [-2.0, -1.0, 0.0, 2.0, 3.0, 2.0, 0.0, -1.0, -2.0, -2.0],
    ),
];

pub type SharedEqState = Arc<RwLock<EqState>>;

pub fn shared_eq_state(state: EqState) -> SharedEqState {
    Arc::new(RwLock::new(state))
}

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn peaking(freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let a = 10_f64.powf(gain_db as f64 / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * freq_hz as f64 / sample_rate as f64;
        let alpha = w0.sin() / (2.0 * q as f64);
        let cos_w0 = w0.cos();

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

pub struct EqProcessor {
    filters: Vec<[Biquad; 2]>,
    last_bands: [f32; EQ_NUM_BANDS],
}

impl EqProcessor {
    pub fn new() -> Self {
        Self {
            filters: vec![[Biquad::peaking(1000.0, 0.0, 1.0, EQ_SAMPLE_RATE); 2]; EQ_NUM_BANDS],
            last_bands: [f32::NAN; EQ_NUM_BANDS],
        }
    }

    fn sync_coefficients(&mut self, bands: &[f32; EQ_NUM_BANDS]) {
        if self.last_bands == *bands {
            return;
        }
        for (i, gain) in bands.iter().enumerate() {
            let freq = EQ_FREQUENCIES_HZ[i];
            self.filters[i] = [
                Biquad::peaking(freq, *gain, 1.0, EQ_SAMPLE_RATE),
                Biquad::peaking(freq, *gain, 1.0, EQ_SAMPLE_RATE),
            ];
        }
        self.last_bands = *bands;
    }

    pub fn process_samples(&mut self, samples: &mut [f64], bands: &[f32; EQ_NUM_BANDS]) {
        self.sync_coefficients(bands);
        for frame in samples.chunks_mut(2) {
            if frame.len() == 2 {
                for (channel, sample) in frame.iter_mut().enumerate() {
                    for filter in &mut self.filters {
                        *sample = filter[channel].process(*sample);
                    }
                }
            }
        }
    }

    pub fn reset(&mut self) {
        for band in &mut self.filters {
            band[0].reset();
            band[1].reset();
        }
    }
}

pub fn process_samples_if_enabled(
    processor: &mut EqProcessor,
    samples: &mut [f64],
    state: &EqState,
) {
    if !state.enabled {
        return;
    }
    if state.bands.iter().all(|g| g.abs() < f32::EPSILON) {
        return;
    }
    processor.process_samples(samples, &state.bands);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_gain_limits_range() {
        assert_eq!(EqState::clamp_gain(-20.0), EQ_MIN_GAIN_DB);
        assert_eq!(EqState::clamp_gain(20.0), EQ_MAX_GAIN_DB);
        assert_eq!(EqState::clamp_gain(3.5), 3.5);
    }

    #[test]
    fn resolve_band_by_name_and_index() {
        assert_eq!(EqState::resolve_band("bass"), Some(1));
        assert_eq!(EqState::resolve_band("3"), Some(3));
        assert_eq!(EqState::resolve_band("unknown"), None);
        assert_eq!(EqState::resolve_band("10"), None);
    }

    #[test]
    fn apply_preset_updates_bands() {
        let mut state = EqState::default();
        assert!(state.apply_preset("bass_boost"));
        assert_eq!(state.bands[1], 5.0);
        assert!(!state.apply_preset("nope"));
    }

    #[test]
    fn process_samples_changes_nonzero_input_when_enabled() {
        let mut processor = EqProcessor::new();
        let mut samples = vec![0.5, 0.5, -0.5, -0.5];
        let state = EqState {
            enabled: true,
            bands: {
                let mut b = [0.0; EQ_NUM_BANDS];
                b[1] = 6.0;
                b
            },
        };
        process_samples_if_enabled(&mut processor, &mut samples, &state);
        assert_ne!(samples, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn disabled_eq_leaves_samples_unchanged() {
        let mut processor = EqProcessor::new();
        let original = vec![0.25, -0.25, 0.1, -0.1];
        let mut samples = original.clone();
        let state = EqState {
            enabled: false,
            bands: [6.0; EQ_NUM_BANDS],
        };
        process_samples_if_enabled(&mut processor, &mut samples, &state);
        assert_eq!(samples, original);
    }
}
