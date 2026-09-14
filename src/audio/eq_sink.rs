use librespot_playback::audio_backend::{Sink, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;

use super::{EqProcessor, SharedEqState, process_samples_if_enabled};

pub struct EqSink {
    inner: Box<dyn Sink>,
    state: SharedEqState,
    processor: EqProcessor,
}

impl EqSink {
    pub fn new(inner: Box<dyn Sink>, state: SharedEqState) -> Self {
        Self {
            inner,
            state,
            processor: EqProcessor::new(),
        }
    }
}

impl Sink for EqSink {
    fn start(&mut self) -> SinkResult<()> {
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.processor.reset();
        self.inner.stop()
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        match packet {
            AudioPacket::Samples(mut samples) => {
                let state = self.state.read().unwrap();
                process_samples_if_enabled(&mut self.processor, &mut samples, &state);
                self.inner.write(AudioPacket::Samples(samples), converter)
            }
            other => self.inner.write(other, converter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    use crate::audio::EqState;

    struct RecordingSink {
        last_len: usize,
    }

    impl Sink for RecordingSink {
        fn write(&mut self, packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
            if let AudioPacket::Samples(samples) = packet {
                self.last_len = samples.len();
            }
            Ok(())
        }
    }

    #[test]
    fn write_forwards_samples_to_inner_sink() {
        let state = Arc::new(RwLock::new(EqState::default()));
        let mut rec = RecordingSink { last_len: 0 };
        let mut converter = Converter::new(None);
        let samples = vec![0.1, -0.1, 0.2, -0.2];
        rec.write(AudioPacket::Samples(samples), &mut converter)
            .unwrap();
        assert_eq!(rec.last_len, 4);

        let mut sink = EqSink::new(Box::new(RecordingSink { last_len: 0 }), state);
        sink.write(
            AudioPacket::Samples(vec![0.1, -0.1, 0.2, -0.2]),
            &mut Converter::new(None),
        )
        .unwrap();
    }
}
