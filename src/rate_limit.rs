//! Rate-limit (HTTP 429) state of the Web API clients, and how it is shown to the user.

use std::fmt;
use std::time::{Duration, Instant};

/// Window assumed when a 429 response has no usable `Retry-After` header, so the throttled state
/// is still visible to the user.
pub const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(5);

/// Which Web API client a rate limit applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientLabel {
    Shared,
    Personal,
}

impl fmt::Display for ClientLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shared => write!(f, "shared"),
            Self::Personal => write!(f, "personal"),
        }
    }
}

/// Parse a `Retry-After` header value given in seconds. Returns `None` if the header is missing
/// or is not a whole number of seconds.
pub fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// When a client's current rate-limit window ends, if it is throttled.
#[derive(Debug, Default)]
pub struct ThrottleState {
    until: Option<Instant>,
}

impl ThrottleState {
    /// Record a 429 response received at `now`. The window ends `retry_after` (or
    /// [DEFAULT_RETRY_AFTER]) from `now`, unless an earlier 429 already set a later end. Returns
    /// true if this starts a new throttle episode, that is the client was not throttled before.
    pub fn record_429(&mut self, now: Instant, retry_after: Option<Duration>) -> bool {
        let new_episode = !self.is_active(now);
        let deadline = now + retry_after.unwrap_or(DEFAULT_RETRY_AFTER);
        self.until = Some(match self.until {
            Some(until) if until > deadline => until,
            _ => deadline,
        });
        new_episode
    }

    /// Record a successful call at `now`. The state is cleared once the window has passed; a
    /// success inside the window (another endpoint, for example) leaves it open.
    pub fn record_success(&mut self, now: Instant) {
        if !self.is_active(now) {
            self.until = None;
        }
    }

    /// Whether the window is still open at `now`.
    pub fn is_active(&self, now: Instant) -> bool {
        self.until.is_some_and(|until| until > now)
    }

    /// Time left in the window at `now`, or `None` if not throttled.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.until
            .map(|until| until.saturating_duration_since(now))
            .filter(|remaining| !remaining.is_zero())
    }
}

/// Whole seconds in `duration`, rounded up so an open window never shows as `0s`.
fn ceil_secs(duration: Duration) -> u64 {
    duration.as_secs() + u64::from(duration.subsec_nanos() > 0)
}

/// The status bar segment for the throttled clients in `status`, for example
/// `[rate limited: personal 12s, shared 42s] `, or an empty string if none is throttled.
pub fn format_status_segment(status: &[(ClientLabel, Duration)]) -> String {
    if status.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = status
        .iter()
        .map(|(label, remaining)| format!("{label} {}s", ceil_secs(*remaining)))
        .collect();
    format!("[rate limited: {}] ", parts.join(", "))
}

/// Body text of the rate-limit desktop notification.
pub fn notification_body(client: ClientLabel, retry_after: Duration) -> String {
    format!(
        "{client} client ID, retrying in {}s",
        ceil_secs(retry_after)
    )
}

#[cfg(test)]
mod test {
    use super::*;

    const SECS: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn parse_retry_after_reads_whole_seconds() {
        assert_eq!(parse_retry_after(Some("30")), Some(SECS(30)));
        assert_eq!(parse_retry_after(Some(" 7 ")), Some(SECS(7)));
    }

    #[test]
    fn parse_retry_after_rejects_missing_and_non_numeric_values() {
        assert_eq!(parse_retry_after(None), None);
        assert_eq!(parse_retry_after(Some("soon")), None);
        assert_eq!(parse_retry_after(Some("1.5")), None);
        assert_eq!(parse_retry_after(Some("")), None);
    }

    #[test]
    fn a_429_opens_a_window_of_retry_after() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, Some(SECS(30)));
        assert_eq!(state.remaining(now), Some(SECS(30)));
        assert!(state.is_active(now + SECS(29)));
        assert!(!state.is_active(now + SECS(30)));
    }

    #[test]
    fn a_shorter_retry_after_keeps_the_later_deadline() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, Some(SECS(30)));
        state.record_429(now + SECS(1), Some(SECS(5)));
        assert_eq!(state.remaining(now), Some(SECS(30)));
    }

    #[test]
    fn a_longer_retry_after_extends_the_deadline() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, Some(SECS(5)));
        state.record_429(now + SECS(1), Some(SECS(30)));
        assert_eq!(state.remaining(now), Some(SECS(31)));
    }

    #[test]
    fn a_missing_retry_after_gives_the_default_window() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, None);
        assert_eq!(state.remaining(now), Some(DEFAULT_RETRY_AFTER));
    }

    #[test]
    fn success_inside_the_window_keeps_it() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, Some(SECS(30)));
        state.record_success(now + SECS(10));
        assert_eq!(state.remaining(now + SECS(10)), Some(SECS(20)));
    }

    #[test]
    fn success_after_the_window_clears_it() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, Some(SECS(30)));
        state.record_success(now + SECS(31));
        assert_eq!(state.remaining(now), None);
        assert!(!state.is_active(now));
    }

    #[test]
    fn not_throttled_by_default() {
        let state = ThrottleState::default();
        assert!(!state.is_active(Instant::now()));
        assert_eq!(state.remaining(Instant::now()), None);
    }

    #[test]
    fn episode_starts_on_first_429_only() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        assert!(state.record_429(now, Some(SECS(30))));
        assert!(!state.record_429(now + SECS(10), Some(SECS(30))));
    }

    #[test]
    fn a_429_after_the_window_starts_a_new_episode() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        assert!(state.record_429(now, Some(SECS(30))));
        assert!(state.record_429(now + SECS(31), Some(SECS(30))));
    }

    #[test]
    fn status_segment_is_empty_when_nothing_is_throttled() {
        assert_eq!(format_status_segment(&[]), "");
    }

    #[test]
    fn status_segment_for_one_client() {
        assert_eq!(
            format_status_segment(&[(ClientLabel::Shared, SECS(42))]),
            "[rate limited: shared 42s] "
        );
    }

    #[test]
    fn status_segment_for_two_clients() {
        assert_eq!(
            format_status_segment(&[
                (ClientLabel::Personal, SECS(12)),
                (ClientLabel::Shared, SECS(42)),
            ]),
            "[rate limited: personal 12s, shared 42s] "
        );
    }

    #[test]
    fn status_segment_rounds_partial_seconds_up() {
        assert_eq!(
            format_status_segment(&[(ClientLabel::Shared, Duration::from_millis(41_200))]),
            "[rate limited: shared 42s] "
        );
    }

    #[test]
    fn notification_body_names_client_and_wait() {
        assert_eq!(
            notification_body(ClientLabel::Personal, SECS(12)),
            "personal client ID, retrying in 12s"
        );
    }
}
