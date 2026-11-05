//! Network calls in flight, and when the last one ended. The drive harness reads this to tell
//! when a view has finished loading; it costs one relaxed atomic operation per call.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::rate_limit::Stamp;

/// Counts calls in flight and remembers when the last one ended.
pub struct Activity {
    in_flight: AtomicUsize,
    /// [Stamp::as_millis] of the last call end; 0 means no call has ended yet.
    last_end_ms: AtomicU64,
}

/// The gauge every Web API and internal API call goes through.
pub static ACTIVITY: Activity = Activity::new();

impl Activity {
    pub const fn new() -> Self {
        Self {
            in_flight: AtomicUsize::new(0),
            last_end_ms: AtomicU64::new(0),
        }
    }

    /// Mark one call as started. The call ends when the guard drops, on every exit path
    /// (return, `?`, panic unwinding). Bind it as `let _call = ...`: `let _ = ...` drops it at
    /// once.
    #[must_use = "the call ends when the guard drops; bind it with `let _call`, not `let _`"]
    pub fn begin(&self) -> CallGuard<'_> {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        CallGuard { activity: self }
    }

    /// Calls currently in flight.
    #[cfg(any(test, feature = "harness"))]
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Time since the last call ended, `None` before the first one ends.
    #[cfg(any(test, feature = "harness"))]
    pub fn quiet_for(&self, now: Stamp) -> Option<std::time::Duration> {
        match self.last_end_ms.load(Ordering::Relaxed) {
            0 => None,
            ms => Some(now.saturating_duration_since(Stamp::from_millis(ms))),
        }
    }
}

/// One call in flight; see [Activity::begin].
pub struct CallGuard<'a> {
    activity: &'a Activity,
}

impl Drop for CallGuard<'_> {
    fn drop(&mut self) {
        // Never panic here: this may run while a panic unwinds, and a second panic aborts.
        if let Some(now) = Stamp::try_now() {
            self.activity
                .last_end_ms
                .store(now.as_millis().max(1), Ordering::Relaxed);
        }
        self.activity.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::time::Duration;

    #[test]
    fn a_call_is_in_flight_until_its_guard_drops() {
        let activity = Activity::new();
        assert_eq!(activity.in_flight(), 0);
        let call = activity.begin();
        assert_eq!(activity.in_flight(), 1);
        let nested = activity.begin();
        assert_eq!(activity.in_flight(), 2);
        drop(nested);
        drop(call);
        assert_eq!(activity.in_flight(), 0);
    }

    #[test]
    fn quiet_for_is_none_until_a_call_has_ended() {
        let activity = Activity::new();
        assert_eq!(activity.quiet_for(Stamp::now()), None);
        let call = activity.begin();
        assert_eq!(activity.quiet_for(Stamp::now()), None, "still in flight");
        drop(call);
        let quiet = activity.quiet_for(Stamp::now()).expect("a call ended");
        assert!(quiet < Duration::from_secs(1));
    }

    #[test]
    fn an_err_return_ends_the_call() {
        fn fails(activity: &Activity) -> Result<(), ()> {
            let _call = activity.begin();
            Err(())
        }
        let activity = Activity::new();
        assert!(fails(&activity).is_err());
        assert_eq!(activity.in_flight(), 0);
    }

    #[test]
    fn an_early_return_ends_the_call() {
        fn returns_early(activity: &Activity, stop: bool) -> u8 {
            let _call = activity.begin();
            if stop {
                return 1;
            }
            2
        }
        let activity = Activity::new();
        assert_eq!(returns_early(&activity, true), 1);
        assert_eq!(activity.in_flight(), 0);
    }

    #[test]
    fn a_panic_ends_the_call() {
        let activity = Activity::new();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _call = activity.begin();
            panic!("conversion failed");
        }));
        assert!(result.is_err());
        assert_eq!(activity.in_flight(), 0);
        assert!(activity.quiet_for(Stamp::now()).is_some());
    }

    #[test]
    fn let_underscore_drops_the_guard_at_once() {
        // Documents the gotcha: `let _ =` does not bind, so the call is over immediately.
        let activity = Activity::new();
        let _ = activity.begin();
        assert_eq!(activity.in_flight(), 0);
    }
}
