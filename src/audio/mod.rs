mod eq;
mod eq_sink;

pub use eq::{
    EQ_BAND_NAMES, EQ_MAX_GAIN_DB, EQ_MIN_GAIN_DB, EQ_NUM_BANDS, EqProcessor, EqState,
    SharedEqState, process_samples_if_enabled, shared_eq_state,
};
pub use eq_sink::EqSink;
