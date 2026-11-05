//! Rate-limit (HTTP 429) state of the Web API clients, and how it is shown to the user.

use std::fmt;
use std::ops::{Add, AddAssign};
use std::sync::{OnceLock, RwLock};
use std::thread::{self, ThreadId};
use std::time::Duration;

use log::{debug, warn};

/// Longest rate-limit window ever recorded, so an absurd `Retry-After` cannot overflow a [Stamp].
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

/// Time between two probe requests through a long rate-limit window. Spotify can send a
/// `Retry-After` of many hours and lift the limit long before it ends, so a background call
/// may send one request through the window this often: a success closes the window, a 429
/// replaces it with the new `Retry-After`. Windows up to this long are never probed.
pub const PROBE_INTERVAL: Duration = Duration::from_secs(5 * 60);

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
    /// Spotify's internal API over the librespot session (per account, no app quota).
    Internal,
}

impl ClientLabel {
    /// Name for log lines and notifications: `internal API`, or `<label> client ID`.
    pub fn describe(self) -> String {
        match self {
            Self::Internal => self.to_string(),
            _ => format!("{self} client ID"),
        }
    }
}

impl fmt::Display for ClientLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shared => write!(f, "shared"),
            Self::Personal => write!(f, "personal"),
            Self::Internal => write!(f, "internal API"),
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

/// A point in time for rate-limit windows, on a clock that keeps running while the machine is
/// suspended. `std::time::Instant` is `CLOCK_MONOTONIC` on Linux, which stops during suspend,
/// so a window open when the lid closes would last the whole suspend longer in real time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp(Duration);

impl Stamp {
    /// The current time on the suspend-aware clock.
    pub fn now() -> Self {
        Self(clock_now())
    }

    /// The current time, or `None` if the clock can't be read. For code that must not panic,
    /// such as a `Drop` that may run during unwinding.
    pub fn try_now() -> Option<Self> {
        try_clock_now().map(Self)
    }

    /// Milliseconds since the clock's origin, for storing a stamp in an atomic.
    pub fn as_millis(self) -> u64 {
        u64::try_from(self.0.as_millis()).unwrap_or(u64::MAX)
    }

    /// The stamp `ms` milliseconds after the clock's origin (inverse of [Self::as_millis]).
    #[cfg(any(test, feature = "harness"))]
    pub fn from_millis(ms: u64) -> Self {
        Self(Duration::from_millis(ms))
    }

    /// `self + duration`, or `None` on overflow.
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.0.checked_add(duration).map(Self)
    }

    /// Time from `earlier` to `self`, or zero if `earlier` is later.
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

impl Add<Duration> for Stamp {
    type Output = Self;

    fn add(self, duration: Duration) -> Self {
        Self(self.0 + duration)
    }
}

impl AddAssign<Duration> for Stamp {
    fn add_assign(&mut self, duration: Duration) {
        self.0 += duration;
    }
}

fn clock_now() -> Duration {
    try_clock_now().expect("clock_gettime(CLOCK_BOOTTIME) failed")
}

/// Time since boot, including time spent suspended.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn try_clock_now() -> Option<Duration> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec. CLOCK_BOOTTIME exists since Linux 2.6.39.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
    (rc == 0).then(|| Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32))
}

/// Time since the first call. Elsewhere, `Instant` is the best portable clock.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn try_clock_now() -> Option<Duration> {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    Some(START.get_or_init(std::time::Instant::now).elapsed())
}

/// Rate-limit state of one client: when its current window ends, when it last got a 429, when
/// it last announced a rate limit to the user, and when a request may next probe the window.
#[derive(Debug, Default)]
pub struct ThrottleState {
    until: Option<Stamp>,
    last_429: Option<Stamp>,
    last_event: Option<Stamp>,
    next_probe: Option<Stamp>,
    probing: bool,
}

impl ThrottleState {
    /// Record a 429 response received at `now`. The window ends `window` (at most
    /// [MAX_RETRY_AFTER]) from `now`, unless an earlier 429 already set a later end; the 429 of
    /// a probe replaces the end instead. Returns true if this starts a new throttle episode,
    /// that is the client was not throttled before.
    pub fn record_429(&mut self, now: Stamp, window: Duration) -> bool {
        let new_episode = !self.is_active(now);
        let window = window.min(MAX_RETRY_AFTER);
        let deadline = now.checked_add(window).unwrap_or(now + MAX_RETRY_AFTER);
        self.until = Some(match self.until {
            Some(until) if until > deadline && !self.probing => until,
            _ => deadline,
        });
        if new_episode || self.probing || self.next_probe.is_none() {
            self.next_probe = Some(now + PROBE_INTERVAL);
        }
        self.probing = false;
        self.last_429 = Some(now);
        new_episode
    }

    /// Record a successful call at `now`. The state is cleared once the window has passed or
    /// when the call was a probe; any other success inside the window (a request sent before
    /// the 429 arrived, for example) leaves it open.
    pub fn record_success(&mut self, now: Stamp) {
        if self.probing || !self.is_active(now) {
            self.until = None;
            self.next_probe = None;
            self.probing = false;
        }
    }

    /// Claim the probe of the window at `now`: true if the window is open and a probe is due,
    /// and then no other probe is due for [PROBE_INTERVAL]. The caller sends one request and
    /// reports it through [Self::record_success], [Self::record_429] or [Self::abandon_probe].
    pub fn claim_probe(&mut self, now: Stamp) -> bool {
        if self.probing || !self.is_active(now) || self.next_probe.is_none_or(|due| due > now) {
            return false;
        }
        self.probing = true;
        self.next_probe = Some(now + PROBE_INTERVAL);
        true
    }

    /// Forget a claimed probe whose request failed for a reason other than a rate limit.
    pub fn abandon_probe(&mut self) {
        self.probing = false;
    }

    /// Whether the window is still open at `now`.
    pub fn is_active(&self, now: Stamp) -> bool {
        self.until.is_some_and(|until| until > now)
    }

    /// Time left in the window at `now`, or `None` if not throttled.
    pub fn remaining(&self, now: Stamp) -> Option<Duration> {
        self.until
            .map(|until| until.saturating_duration_since(now))
            .filter(|remaining| !remaining.is_zero())
    }

    /// Whether a 429 was recorded at or after `since`, even if its window has closed since.
    pub fn rate_limited_since(&self, since: Stamp) -> bool {
        self.last_429.is_some_and(|at| at >= since)
    }

    /// Whether a rate limit should be announced to the user at `now`: true, and remembered, if
    /// nothing was announced in the last [NOTIFY_COOLDOWN].
    pub fn should_announce(&mut self, now: Stamp) -> bool {
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
        "{}, retrying in {}s",
        client.describe(),
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

/// Outcome of one Web API request, as far as rate limiting cares.
pub enum Attempt<R> {
    /// The request succeeded.
    Done(R),
    /// HTTP 429, with the `Retry-After` value if the server sent a usable one.
    RateLimited(Option<Duration>),
    /// Any other failure, with the HTTP status if the server sent one.
    Failed(Option<u16>),
}

/// Run a Web API request on a client whose rate-limit state is `throttle`, with bounded
/// rate-limit handling:
///
/// - before every request, [gate] decides: send, wait (background threads, short windows
///   only), or fail without sending; a background call that would fail sends instead when it
///   can claim the window's probe ([ThrottleState::claim_probe]);
/// - a 429 calls `on_429` with the window to record (`Retry-After`, or [backoff] if missing),
///   then the gate decides again; after [MAX_RATE_LIMIT_RETRIES] retries the call fails.
///
/// A failed call returns the HTTP status if there was one; 429 when it failed on a rate limit.
pub fn run_rate_limited<R>(
    label: ClientLabel,
    throttle: &RwLock<ThrottleState>,
    on_ui_thread: bool,
    jitter: impl Fn() -> Duration,
    mut send: impl FnMut() -> Attempt<R>,
    mut on_429: impl FnMut(Duration),
) -> Result<R, Option<u16>> {
    let mut retries = 0;
    loop {
        let probing = loop {
            let now = Stamp::now();
            let remaining = throttle.read().unwrap().remaining(now);
            match gate(remaining, on_ui_thread, jitter()) {
                Gate::Send => break false,
                Gate::Wait(delay) => thread::sleep(delay),
                Gate::FailFast if !on_ui_thread && throttle.write().unwrap().claim_probe(now) => {
                    debug!("probing the rate-limit window of the {}", label.describe());
                    break true;
                }
                Gate::FailFast => {
                    debug!("rate limited on the {}, request not sent", label.describe());
                    return Err(Some(429));
                }
            }
        };
        match send() {
            Attempt::Done(value) => {
                throttle.write().unwrap().record_success(Stamp::now());
                return Ok(value);
            }
            Attempt::RateLimited(retry_after) => {
                // A zero Retry-After opens no window, so treat it as missing.
                on_429(
                    retry_after
                        .filter(|d| !d.is_zero())
                        .unwrap_or_else(|| backoff(retries)),
                );
                if on_ui_thread {
                    debug!(
                        "rate limited on the {}, UI thread call not retried",
                        label.describe()
                    );
                    return Err(Some(429));
                }
                if retries == MAX_RATE_LIMIT_RETRIES {
                    warn!(
                        "giving up on a call to the {} after \
                         {MAX_RATE_LIMIT_RETRIES} rate-limited retries",
                        label.describe()
                    );
                    return Err(Some(429));
                }
                retries += 1;
            }
            Attempt::Failed(status) => {
                if probing {
                    throttle.write().unwrap().abandon_probe();
                }
                return Err(status);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const SECS: fn(u64) -> Duration = Duration::from_secs;
    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn describe_adds_client_id_except_for_internal() {
        assert_eq!(ClientLabel::Shared.describe(), "shared client ID");
        assert_eq!(ClientLabel::Personal.describe(), "personal client ID");
        assert_eq!(ClientLabel::Internal.describe(), "internal API");
    }

    #[test]
    fn internal_label_is_named_without_client_id() {
        assert_eq!(ClientLabel::Internal.to_string(), "internal API");
        assert_eq!(
            notification_body(ClientLabel::Internal, SECS(5)),
            "internal API, retrying in 5s"
        );
        assert_eq!(
            format_status_segment(&[(ClientLabel::Internal, SECS(5))]),
            "[rate limited: internal API 5s] "
        );
    }

    #[test]
    fn a_huge_retry_after_is_capped_and_does_not_panic() {
        let now = Stamp::now();
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
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
        assert_eq!(state.remaining(now), Some(SECS(30)));
        assert!(state.is_active(now + SECS(29)));
        assert!(!state.is_active(now + SECS(30)));
    }

    #[test]
    fn a_shorter_retry_after_keeps_the_later_deadline() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
        state.record_429(now + SECS(1), SECS(5));
        assert_eq!(state.remaining(now), Some(SECS(30)));
    }

    #[test]
    fn a_longer_retry_after_extends_the_deadline() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(5));
        state.record_429(now + SECS(1), SECS(30));
        assert_eq!(state.remaining(now), Some(SECS(31)));
    }

    #[test]
    fn success_inside_the_window_keeps_it() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
        state.record_success(now + SECS(10));
        assert_eq!(state.remaining(now + SECS(10)), Some(SECS(20)));
    }

    #[test]
    fn success_after_the_window_clears_it() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(30));
        state.record_success(now + SECS(31));
        assert_eq!(state.remaining(now), None);
        assert!(!state.is_active(now));
    }

    #[test]
    fn not_throttled_by_default() {
        let state = ThrottleState::default();
        assert!(!state.is_active(Stamp::now()));
        assert_eq!(state.remaining(Stamp::now()), None);
    }

    #[test]
    fn episode_starts_on_first_429_only() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        assert!(state.record_429(now, SECS(30)));
        assert!(!state.record_429(now + SECS(10), SECS(30)));
    }

    #[test]
    fn a_429_after_the_window_starts_a_new_episode() {
        let now = Stamp::now();
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
        let start = Stamp::now();
        let mut state = ThrottleState::default();
        assert!(!state.rate_limited_since(start));
        state.record_429(start + SECS(1), SECS(5));
        assert!(state.rate_limited_since(start));
        assert!(state.rate_limited_since(start + SECS(1)));
        assert!(!state.rate_limited_since(start + SECS(2)));
    }

    #[test]
    fn rate_limited_since_survives_the_window_closing() {
        let start = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(start, SECS(1));
        state.record_success(start + SECS(5));
        assert!(!state.is_active(start + SECS(5)));
        assert!(state.rate_limited_since(start));
    }

    #[test]
    fn should_announce_once_per_cooldown() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        assert!(state.should_announce(now));
        assert!(!state.should_announce(now + SECS(1)));
        assert!(!state.should_announce(now + (NOTIFY_COOLDOWN - MS(1))));
        assert!(state.should_announce(now + NOTIFY_COOLDOWN));
    }

    #[test]
    fn back_to_back_backoff_episodes_announce_once() {
        let now = Stamp::now();
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

    use std::cell::Cell;

    fn no_jitter() -> Duration {
        Duration::ZERO
    }

    #[test]
    fn run_sends_once_when_not_throttled() {
        let throttle = RwLock::new(ThrottleState::default());
        let calls = Cell::new(0);
        let result = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::Done(7)
            },
            |_| {},
        );
        assert_eq!(result, Ok(7));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn run_sends_nothing_inside_a_long_window() {
        let throttle = RwLock::new(ThrottleState::default());
        throttle.write().unwrap().record_429(Stamp::now(), SECS(60));
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Personal,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::Done(())
            },
            |_| {},
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn run_on_the_ui_thread_sends_nothing_inside_a_short_window() {
        let throttle = RwLock::new(ThrottleState::default());
        throttle.write().unwrap().record_429(Stamp::now(), SECS(1));
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            true,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::Done(())
            },
            |_| {},
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn run_waits_out_a_short_window_in_the_background() {
        let throttle = RwLock::new(ThrottleState::default());
        let start = std::time::Instant::now();
        throttle.write().unwrap().record_429(Stamp::now(), MS(50));
        let result = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || Attempt::Done("ok"),
            |_| {},
        );
        assert_eq!(result, Ok("ok"));
        assert!(start.elapsed() >= MS(50));
    }

    #[test]
    fn run_gives_up_after_the_retry_limit() {
        let throttle = RwLock::new(ThrottleState::default());
        let calls = Cell::new(0u32);
        // on_429 does not record a window, so no test time is spent waiting.
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::RateLimited(None)
            },
            |_| {},
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 1 + MAX_RATE_LIMIT_RETRIES);
    }

    #[test]
    fn run_uses_backoff_without_retry_after_and_the_header_otherwise() {
        let throttle = RwLock::new(ThrottleState::default());
        let windows = std::cell::RefCell::new(Vec::new());
        let _: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || Attempt::RateLimited(None),
            |window| windows.borrow_mut().push(window),
        );
        assert_eq!(*windows.borrow(), vec![SECS(1), SECS(2), SECS(4), SECS(8)]);

        windows.borrow_mut().clear();
        let _: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || Attempt::RateLimited(Some(SECS(3))),
            |window| windows.borrow_mut().push(window),
        );
        assert_eq!(windows.borrow()[0], SECS(3));
    }

    #[test]
    fn run_on_the_ui_thread_does_not_retry_a_429() {
        let throttle = RwLock::new(ThrottleState::default());
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            true,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::RateLimited(Some(SECS(2)))
            },
            |window| {
                throttle.write().unwrap().record_429(Stamp::now(), window);
            },
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn run_on_the_ui_thread_does_not_retry_a_zero_retry_after() {
        let throttle = RwLock::new(ThrottleState::default());
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            true,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::RateLimited(Some(Duration::ZERO))
            },
            |window| {
                throttle.write().unwrap().record_429(Stamp::now(), window);
            },
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn run_treats_a_zero_retry_after_as_missing() {
        let throttle = RwLock::new(ThrottleState::default());
        let windows = std::cell::RefCell::new(Vec::new());
        let _: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || Attempt::RateLimited(Some(Duration::ZERO)),
            |window| windows.borrow_mut().push(window),
        );
        assert_eq!(windows.borrow()[0], backoff(0));
        assert_eq!(windows.borrow()[0], SECS(1));
    }

    #[test]
    fn run_succeeds_after_a_429_and_clears_the_window() {
        let throttle = RwLock::new(ThrottleState::default());
        let calls = Cell::new(0);
        let result = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    Attempt::RateLimited(None)
                } else {
                    Attempt::Done("ok")
                }
            },
            |_| {},
        );
        assert_eq!(result, Ok("ok"));
        assert_eq!(calls.get(), 2);
        assert_eq!(throttle.read().unwrap().remaining(Stamp::now()), None);
    }

    #[test]
    fn run_passes_other_failures_through() {
        let throttle = RwLock::new(ThrottleState::default());
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Shared,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::Failed(Some(500))
            },
            |_| {},
        );
        assert_eq!(result, Err(Some(500)));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn stamps_move_forward_and_add_durations() {
        let a = Stamp::now();
        let b = Stamp::now();
        assert!(b >= a);
        assert_eq!((a + SECS(5)).saturating_duration_since(a), SECS(5));
        assert_eq!(a.saturating_duration_since(a + SECS(5)), Duration::ZERO);
    }

    #[test]
    fn stamps_round_trip_through_milliseconds() {
        let stamp = Stamp::from_millis(1_234);
        assert_eq!(stamp.as_millis(), 1_234);
        assert_eq!(
            Stamp::from_millis(1_500).saturating_duration_since(stamp),
            Duration::from_millis(266)
        );
    }

    #[test]
    fn try_now_reads_the_same_clock_as_now() {
        let before = Stamp::now();
        let read = Stamp::try_now().expect("the clock is readable");
        assert!(read >= before);
        assert!(Stamp::now() >= read);
    }

    #[test]
    fn a_long_window_allows_one_probe_per_interval() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(3600));
        assert!(!state.claim_probe(now));
        assert!(!state.claim_probe(now + (PROBE_INTERVAL - MS(1))));
        assert!(state.claim_probe(now + PROBE_INTERVAL));
        assert!(!state.claim_probe(now + PROBE_INTERVAL));
        state.abandon_probe();
        assert!(state.claim_probe(now + PROBE_INTERVAL * 2));
    }

    #[test]
    fn a_window_shorter_than_the_probe_interval_is_never_probed() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, PROBE_INTERVAL);
        assert!(!state.claim_probe(now + PROBE_INTERVAL));
    }

    #[test]
    fn a_successful_probe_clears_the_window() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(3600));
        assert!(state.claim_probe(now + PROBE_INTERVAL));
        state.record_success(now + PROBE_INTERVAL);
        assert!(!state.is_active(now + PROBE_INTERVAL));
    }

    #[test]
    fn a_rate_limited_probe_replaces_the_deadline() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(3600));
        assert!(state.claim_probe(now + PROBE_INTERVAL));
        assert!(!state.record_429(now + PROBE_INTERVAL, SECS(60)));
        assert_eq!(state.remaining(now + PROBE_INTERVAL), Some(SECS(60)));
    }

    #[test]
    fn an_abandoned_probe_does_not_clear_the_window_on_a_later_success() {
        let now = Stamp::now();
        let mut state = ThrottleState::default();
        state.record_429(now, SECS(3600));
        assert!(state.claim_probe(now + PROBE_INTERVAL));
        state.abandon_probe();
        state.record_success(now + PROBE_INTERVAL);
        assert!(state.is_active(now + PROBE_INTERVAL));
    }

    /// A throttle whose window of `window` opened now, with its probe already due. Not opened
    /// one probe interval in the past: the clock counts from boot, so on a machine up for less
    /// than that (a fresh CI runner) the stamp would clamp to zero and no probe would be due.
    fn throttle_with_probe_due(window: Duration) -> RwLock<ThrottleState> {
        let throttle = RwLock::new(ThrottleState::default());
        let now = Stamp::now();
        let mut state = throttle.write().unwrap();
        state.record_429(now, window);
        state.next_probe = Some(now);
        drop(state);
        throttle
    }

    #[test]
    fn run_probes_a_long_window_from_the_background_and_clears_it() {
        let throttle = throttle_with_probe_due(SECS(3600));
        let calls = Cell::new(0);
        let result = run_rate_limited(
            ClientLabel::Personal,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::Done("ok")
            },
            |_| {},
        );
        assert_eq!(result, Ok("ok"));
        assert_eq!(calls.get(), 1);
        assert_eq!(throttle.read().unwrap().remaining(Stamp::now()), None);
    }

    #[test]
    fn run_never_probes_from_the_ui_thread() {
        let throttle = throttle_with_probe_due(SECS(3600));
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Personal,
            &throttle,
            true,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::Done(())
            },
            |_| {},
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 0);
        assert!(throttle.read().unwrap().is_active(Stamp::now()));
    }

    #[test]
    fn run_sends_one_probe_and_records_its_new_window() {
        let throttle = throttle_with_probe_due(SECS(3600));
        let calls = Cell::new(0);
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Personal,
            &throttle,
            false,
            no_jitter,
            || {
                calls.set(calls.get() + 1);
                Attempt::RateLimited(Some(SECS(120)))
            },
            |window| {
                throttle.write().unwrap().record_429(Stamp::now(), window);
            },
        );
        assert_eq!(result, Err(Some(429)));
        assert_eq!(calls.get(), 1);
        let remaining = throttle.read().unwrap().remaining(Stamp::now()).unwrap();
        assert!(remaining <= SECS(120) && remaining > SECS(110));
    }

    #[test]
    fn run_abandons_a_probe_that_fails_otherwise() {
        let throttle = throttle_with_probe_due(SECS(3600));
        let result: Result<(), _> = run_rate_limited(
            ClientLabel::Personal,
            &throttle,
            false,
            no_jitter,
            || Attempt::Failed(Some(500)),
            |_| {},
        );
        assert_eq!(result, Err(Some(500)));
        let mut state = throttle.write().unwrap();
        state.record_success(Stamp::now());
        assert!(state.is_active(Stamp::now()));
    }
}
