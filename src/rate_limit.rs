//! Rate-limit (HTTP 429) state of the Web API clients, and how it is shown to the user.

use std::fmt;
use std::sync::OnceLock;
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

/// Window assumed when a 429 response has no usable `Retry-After` header, so the throttled state
/// is still visible to the user.
pub const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(5);

/// Longest rate-limit window ever recorded, so an absurd `Retry-After` cannot overflow `Instant`.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// Longest rate-limit window a background call waits through before retrying. Longer windows
/// fail the call at once.
pub const WAIT_THRESHOLD: Duration = Duration::from_secs(10);

/// Upper bound of the random delay added to a wait, so threads waiting on the same window do
/// not all retry at the same instant.
pub const MAX_JITTER: Duration = Duration::from_millis(1000);

/// Retries of one call after a 429, so a call makes at most `1 + MAX_RATE_LIMIT_RETRIES`
/// requests.
pub const MAX_RATE_LIMIT_RETRIES: u32 = 3;

/// First backoff window when a 429 has no usable `Retry-After`; doubled on every retry.
pub const BACKOFF_BASE: Duration = Duration::from_secs(1);

/// Consecutive automatic library sync resumes after rate limits.
pub const MAX_SYNC_RESUMES: u32 = 5;

/// Shortest wait before a resumed library sync. Spotify counts requests over a rolling 30s
/// window.
pub const RESUME_MIN_DELAY: Duration = Duration::from_secs(30);

/// Shortest time between two rate-limit events (log line and desktop notification) for one
/// client.
pub const NOTIFY_COOLDOWN: Duration = Duration::from_secs(60);

/// What to do with a request on a client, given its rate-limit window.
#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    /// Send the request now.
    Send,
    /// Sleep this long, then decide again.
    Wait(Duration),
    /// Do not send; fail the call.
    FailFast,
}

/// Decide whether to send a request on a client whose window has `remaining` time left. The UI
/// thread never waits. Background threads wait through windows up to [WAIT_THRESHOLD], plus
/// `jitter`, and fail at once on longer ones.
pub fn gate(remaining: Option<Duration>, on_ui_thread: bool, jitter: Duration) -> Gate {
    match remaining {
        None => Gate::Send,
        Some(_) if on_ui_thread => Gate::FailFast,
        Some(window) if window <= WAIT_THRESHOLD => Gate::Wait(window + jitter),
        Some(_) => Gate::FailFast,
    }
}

/// Window to assume after a 429 without a usable `Retry-After`, when `retries` retries of the
/// call were already made: [BACKOFF_BASE] doubled each time, capped at [WAIT_THRESHOLD].
pub fn backoff(retries: u32) -> Duration {
    let factor = 2u32.checked_pow(retries).unwrap_or(u32::MAX);
    BACKOFF_BASE.saturating_mul(factor).min(WAIT_THRESHOLD)
}

/// A random delay between zero and [MAX_JITTER].
pub fn random_jitter() -> Duration {
    Duration::from_millis(rand::random_range(0..=MAX_JITTER.as_millis() as u64))
}

/// The thread that runs the cursive event loop, set once at startup.
static UI_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Record the current thread as the UI thread. Later calls do nothing.
pub fn set_ui_thread() {
    let _ = UI_THREAD.set(thread::current().id());
}

/// Whether the current thread is the UI thread. False if [set_ui_thread] was never called.
pub fn is_ui_thread() -> bool {
    UI_THREAD
        .get()
        .is_some_and(|id| *id == thread::current().id())
}

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

/// Rate-limit state of one client: when its current window ends, when it last got a 429, and
/// when it last announced a rate limit to the user.
#[derive(Debug, Default)]
pub struct ThrottleState {
    until: Option<Instant>,
    last_429: Option<Instant>,
    last_event: Option<Instant>,
}

impl ThrottleState {
    /// Record a 429 response received at `now`. The window ends `window` (at most
    /// [MAX_RETRY_AFTER]) from `now`, unless an earlier 429 already set a later end. Returns
    /// true if this starts a new throttle episode, that is the client was not throttled before.
    pub fn record_429(&mut self, now: Instant, window: Duration) -> bool {
        let new_episode = !self.is_active(now);
        let window = window.min(MAX_RETRY_AFTER);
        let deadline = now.checked_add(window).unwrap_or(now + MAX_RETRY_AFTER);
        self.until = Some(match self.until {
            Some(until) if until > deadline => until,
            _ => deadline,
        });
        self.last_429 = Some(now);
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

    /// Whether a 429 was recorded at or after `since`, even if its window has closed since.
    pub fn rate_limited_since(&self, since: Instant) -> bool {
        self.last_429.is_some_and(|at| at >= since)
    }

    /// Whether a rate limit should be announced to the user at `now`: true, and remembered, if
    /// nothing was announced in the last [NOTIFY_COOLDOWN].
    pub fn should_announce(&mut self, now: Instant) -> bool {
        if self
            .last_event
            .is_some_and(|at| now.saturating_duration_since(at) < NOTIFY_COOLDOWN)
        {
            return false;
        }
        self.last_event = Some(now);
        true
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
#[cfg_attr(not(feature = "notify"), allow(dead_code))]
pub fn notification_body(client: ClientLabel, retry_after: Duration) -> String {
    format!(
        "{client} client ID, retrying in {}s",
        ceil_secs(retry_after)
    )
}

/// ID of the last warning notification (rate limit, personal client disabled), so a new one
/// replaces it instead of stacking. Separate from the now-playing notification so neither
/// overwrites the other.
#[cfg(feature = "notify")]
static WARNING_NOTIFICATION_ID: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);

/// Show a warning desktop notification. Blocks on the notification service, so call it from a
/// separate thread.
#[cfg(feature = "notify")]
pub fn send_warning_notification(summary: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.appname("ncspot").summary(summary).body(body);

    // Normal urgency and not transient, so the notification doesn't vanish unseen.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        n.urgency(notify_rust::Urgency::Normal)
            .hint(notify_rust::Hint::DesktopEntry("ncspot".into()));
        if let Some(id) = *WARNING_NOTIFICATION_ID.lock().unwrap() {
            n.id(id);
        }
    }

    match n.show() {
        Ok(handle) => {
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                *WARNING_NOTIFICATION_ID.lock().unwrap() = Some(handle.id());
            }
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            drop(handle);
        }
        Err(e) => log::error!("Failed to send warning notification: {e}"),
    }
}

/// Show a desktop notification that `client` is rate limited for `retry_after`.
#[cfg(feature = "notify")]
pub fn send_rate_limit_notification(client: ClientLabel, retry_after: Duration) {
    send_warning_notification(
        "ncspot: rate limited by Spotify",
        &notification_body(client, retry_after),
    );
}

#[cfg(test)]
mod test {
    use super::*;

    const SECS: fn(u64) -> Duration = Duration::from_secs;
    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn a_huge_retry_after_is_capped_and_does_not_panic() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, Duration::from_secs(u64::MAX));
        assert_eq!(state.remaining(now), Some(MAX_RETRY_AFTER));
    }

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
        state.record_429(now, SECS(30));
        assert_eq!(state.remaining(now), Some(SECS(30)));
        assert!(state.is_active(now + SECS(29)));
        assert!(!state.is_active(now + SECS(30)));
    }

    #[test]
    fn a_shorter_retry_after_keeps_the_later_deadline() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
        state.record_429(now + SECS(1), SECS(5));
        assert_eq!(state.remaining(now), Some(SECS(30)));
    }

    #[test]
    fn a_longer_retry_after_extends_the_deadline() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(5));
        state.record_429(now + SECS(1), SECS(30));
        assert_eq!(state.remaining(now), Some(SECS(31)));
    }

    #[test]
    fn success_inside_the_window_keeps_it() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
        state.record_success(now + SECS(10));
        assert_eq!(state.remaining(now + SECS(10)), Some(SECS(20)));
    }

    #[test]
    fn success_after_the_window_clears_it() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
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
        assert!(state.record_429(now, SECS(30)));
        assert!(!state.record_429(now + SECS(10), SECS(30)));
    }

    #[test]
    fn a_429_after_the_window_starts_a_new_episode() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        assert!(state.record_429(now, SECS(30)));
        assert!(state.record_429(now + SECS(31), SECS(30)));
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

    #[test]
    fn gate_sends_when_not_throttled() {
        assert_eq!(gate(None, false, MS(300)), Gate::Send);
        assert_eq!(gate(None, true, MS(300)), Gate::Send);
    }

    #[test]
    fn gate_never_waits_on_the_ui_thread() {
        assert_eq!(gate(Some(MS(1)), true, MS(300)), Gate::FailFast);
        assert_eq!(gate(Some(WAIT_THRESHOLD), true, MS(300)), Gate::FailFast);
        assert_eq!(gate(Some(SECS(60)), true, MS(300)), Gate::FailFast);
    }

    #[test]
    fn gate_waits_through_short_windows_in_the_background() {
        assert_eq!(gate(Some(SECS(3)), false, MS(300)), Gate::Wait(MS(3300)));
        assert_eq!(
            gate(Some(WAIT_THRESHOLD), false, MS(0)),
            Gate::Wait(WAIT_THRESHOLD)
        );
    }

    #[test]
    fn gate_fails_fast_on_long_windows_in_the_background() {
        assert_eq!(
            gate(Some(WAIT_THRESHOLD + MS(1)), false, MS(300)),
            Gate::FailFast
        );
        assert_eq!(gate(Some(SECS(3600)), false, MS(300)), Gate::FailFast);
    }

    #[test]
    fn backoff_doubles_and_is_capped() {
        let delays: Vec<Duration> = (0..6).map(backoff).collect();
        assert_eq!(
            delays,
            vec![SECS(1), SECS(2), SECS(4), SECS(8), SECS(10), SECS(10)]
        );
        assert_eq!(backoff(u32::MAX), WAIT_THRESHOLD);
    }

    #[test]
    fn random_jitter_stays_in_range() {
        for _ in 0..100 {
            assert!(random_jitter() <= MAX_JITTER);
        }
    }

    #[test]
    fn ui_thread_is_only_the_thread_that_set_it() {
        // UI_THREAD is process-wide, so this is the only test that sets it. It does so from a
        // thread of its own; ThreadIds are never reused, so no other test thread can match.
        let set_here = std::thread::spawn(|| {
            set_ui_thread();
            is_ui_thread()
        })
        .join()
        .unwrap();
        assert!(set_here);
        assert!(!is_ui_thread());
    }

    #[test]
    fn rate_limited_since_tracks_the_last_429() {
        let start = Instant::now();
        let mut state = ThrottleState::default();
        assert!(!state.rate_limited_since(start));
        state.record_429(start + SECS(1), SECS(5));
        assert!(state.rate_limited_since(start));
        assert!(state.rate_limited_since(start + SECS(1)));
        assert!(!state.rate_limited_since(start + SECS(2)));
    }

    #[test]
    fn rate_limited_since_survives_the_window_closing() {
        let start = Instant::now();
        let mut state = ThrottleState::default();
        state.record_429(start, SECS(1));
        state.record_success(start + SECS(5));
        assert!(!state.is_active(start + SECS(5)));
        assert!(state.rate_limited_since(start));
    }

    #[test]
    fn should_announce_once_per_cooldown() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        assert!(state.should_announce(now));
        assert!(!state.should_announce(now + SECS(1)));
        assert!(!state.should_announce(now + NOTIFY_COOLDOWN - MS(1)));
        assert!(state.should_announce(now + NOTIFY_COOLDOWN));
    }

    #[test]
    fn back_to_back_backoff_episodes_announce_once() {
        let now = Instant::now();
        let mut state = ThrottleState::default();
        let mut announced = 0;
        let mut t = now;
        for retries in 0..3 {
            let window = backoff(retries);
            if state.record_429(t, window) && state.should_announce(t) {
                announced += 1;
            }
            // The next 429 arrives after this window has closed: a new episode each time.
            t += window + MS(10);
        }
        assert_eq!(announced, 1);
    }
}
