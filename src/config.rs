use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard};
use std::{fs, process};

use cursive::theme::Theme;
use log::{debug, error, warn};
use ncspot::{CONFIGURATION_FILE_NAME, USER_STATE_FILE_NAME};
use platform_dirs::AppDirs;

use crate::client_id::{self, ClientIdSource, PersonalClientId};
use crate::command::{SortDirection, SortKey};
use crate::model::playable::Playable;
use crate::queue;
use crate::serialization::{CBOR, Serializer, TOML};

pub const CACHE_VERSION: u16 = 1;
pub const DEFAULT_COMMAND_KEY: char = ':';

/// The playback state when ncspot is started.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    Default,
}

/// The focussed library tab when ncspot is started.
#[derive(Clone, Serialize, Deserialize, Debug, Hash, strum_macros::EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum LibraryTab {
    Tracks,
    Albums,
    Artists,
    Playlists,
    Podcasts,
    Browse,
}

/// The format used to represent tracks in a list.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct TrackFormat {
    pub left: Option<String>,
    pub center: Option<String>,
    pub right: Option<String>,
}

impl TrackFormat {
    pub fn default() -> Self {
        Self {
            left: Some(String::from("%artists - %title")),
            center: Some(String::from("%album")),
            right: Some(String::from("%saved %duration")),
        }
    }
}

/// The format used when sending desktop notifications about playback status.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct NotificationFormat {
    pub title: Option<String>,
    pub body: Option<String>,
}

impl NotificationFormat {
    pub fn default() -> Self {
        Self {
            title: Some(String::from("%title")),
            body: Some(String::from("%artists")),
        }
    }
}

/// The configuration of ncspot.
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ConfigValues {
    pub command_key: Option<char>,
    pub initial_screen: Option<String>,
    pub default_keybindings: Option<bool>,
    pub keybindings: Option<HashMap<String, String>>,
    pub theme: Option<ConfigTheme>,
    pub use_nerdfont: Option<bool>,
    pub flip_status_indicators: Option<bool>,
    pub audio_cache: Option<bool>,
    pub audio_cache_size: Option<u32>,
    pub backend: Option<String>,
    pub backend_device: Option<String>,
    pub volnorm: Option<bool>,
    pub volnorm_pregain: Option<f64>,
    pub notify: Option<bool>,
    /// Show a desktop notification when Spotify rate limits ncspot (HTTP 429). Defaults to true.
    pub notify_rate_limit: Option<bool>,
    pub bitrate: Option<u32>,
    pub gapless: Option<bool>,
    pub shuffle: Option<bool>,
    pub repeat: Option<queue::RepeatSetting>,
    pub cover_max_scale: Option<f32>,
    pub playback_state: Option<PlaybackState>,
    pub track_format: Option<TrackFormat>,
    pub notification_format: Option<NotificationFormat>,
    pub statusbar_format: Option<String>,
    pub library_tabs: Option<Vec<LibraryTab>>,
    pub hide_display_names: Option<bool>,
    pub ap_port: Option<u16>,
    /// Personal Spotify Web API client ID. Overridden by the `NCSPOT_CLIENT_ID` environment
    /// variable and the `--client-id` flag.
    pub client_id: Option<String>,
    /// Proxy used for all of ncspot's network traffic: the Spotify access
    /// point, the Spotify APIs, OAuth and cover art downloads. Supported
    /// schemes: `http://` (CONNECT proxy) and `socks5h://` (SOCKS5 proxy with
    /// remote DNS resolution). Both accept `user:password@host:port`.
    pub proxy: Option<String>,
    pub lyrics: Option<LyricsConfig>,
    pub jukebox: Option<JukeboxConfig>,
}

/// Configuration for the lyrics feature.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct LyricsConfig {
    pub enabled: Option<bool>,
    pub providers: Option<Vec<String>>,
    pub musixmatch_token: Option<String>,
    pub show_translation: Option<bool>,
    pub show_romaji: Option<bool>,
    pub romanize_client_side: Option<bool>,
    pub persist_offset: Option<bool>,
    pub allow_scroll: Option<bool>,
    pub allow_offset: Option<bool>,
    pub allow_provider_switch: Option<bool>,
    pub allow_copy: Option<bool>,
    pub allow_seek: Option<bool>,
}

impl LyricsConfig {
    pub fn provider_order(&self) -> Vec<crate::lyrics::model::ProviderId> {
        use crate::lyrics::model::ProviderId;
        match &self.providers {
            Some(list) => list
                .iter()
                .filter_map(|s| ProviderId::from_str(s))
                .collect(),
            None => vec![
                ProviderId::Lrclib,
                ProviderId::Spotify,
                ProviderId::Netease,
                ProviderId::Musixmatch,
            ],
        }
    }
}

/// Configuration for the Eternal Jukebox feature.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct JukeboxConfig {
    pub enabled: Option<bool>,
    pub branch_similarity_threshold: Option<u32>,
    pub dynamic_threshold: Option<bool>,
    pub min_branch_probability: Option<f64>,
    pub max_branch_probability: Option<f64>,
    pub branch_probability_ramp: Option<f64>,
    pub only_backward_branches: Option<bool>,
    pub only_long_branches: Option<bool>,
    pub remove_sequential_branches: Option<bool>,
    pub add_last_branch: Option<bool>,
    pub always_follow_last_branch: Option<bool>,
    pub max_play_time_secs: Option<u64>,
    pub analysis_sources: Option<Vec<String>>,
    pub eternalbox_url: Option<String>,
    // Visualizer dials.
    pub show_all_branches: Option<bool>,
    pub max_branches_drawn: Option<usize>,
    pub branch_layouts: Option<Vec<String>>,
    pub graphics: Option<bool>,
    pub graphics_max_px: Option<usize>,
    // Anti-loop.
    pub break_loops: Option<bool>,
    pub loop_threshold: Option<usize>,
    pub loop_identity: Option<String>,
    pub loop_count_mode: Option<String>,
    pub loop_skip_action: Option<String>,
    pub break_last_branch: Option<bool>,
    pub loop_counter: Option<String>,
    /// Default for the global "override per-song settings" toggle on first run.
    pub override_per_song: Option<bool>,
}

impl JukeboxConfig {
    /// Ordered analysis source ids, defaulting to spclient then the eternalbox fallback.
    pub fn analysis_source_order(&self) -> Vec<String> {
        match &self.analysis_sources {
            Some(list) if !list.is_empty() => list.clone(),
            _ => vec!["spclient".to_string(), "eternalbox".to_string()],
        }
    }
}

/// The ncspot theme.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ConfigTheme {
    pub background: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub title: Option<String>,
    pub playing: Option<String>,
    pub playing_selected: Option<String>,
    pub playing_bg: Option<String>,
    pub highlight: Option<String>,
    pub highlight_bg: Option<String>,
    pub highlight_inactive_bg: Option<String>,
    pub error: Option<String>,
    pub error_bg: Option<String>,
    pub statusbar_progress: Option<String>,
    pub statusbar_progress_bg: Option<String>,
    pub statusbar: Option<String>,
    pub statusbar_bg: Option<String>,
    pub statusbar_warning: Option<String>,
    pub cmdline: Option<String>,
    pub cmdline_bg: Option<String>,
    pub search_match: Option<String>,
    pub lyrics_highlight: Option<String>,
    pub lyrics_secondary: Option<String>,
    pub jukebox_branch: Option<String>,
    pub jukebox_cursor: Option<String>,
}

/// The ordering that is used when representing a playlist.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SortingOrder {
    pub key: SortKey,
    pub direction: SortDirection,
}

/// The runtime state of the music queue.
#[derive(Serialize, Default, Deserialize, Debug, Clone)]
pub struct QueueState {
    pub current_track: Option<usize>,
    pub random_order: Option<Vec<usize>>,
    pub track_progress: std::time::Duration,
    pub queue: Vec<Playable>,
}

/// Runtime state that should be persisted accross sessions.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserState {
    pub volume: u16,
    pub shuffle: bool,
    pub repeat: queue::RepeatSetting,
    pub queuestate: QueueState,
    pub playlist_orders: HashMap<String, SortingOrder>,
    pub cache_version: u16,
    pub playback_state: PlaybackState,
    /// Jukebox tuning settings persisted from the live settings modal. When present this
    /// overrides the `[jukebox]` config; the modal's Reset clears it. Never holds the
    /// endless-mode / graphics on/off toggles, so the jukebox always starts disabled.
    #[serde(default)]
    pub jukebox: Option<crate::jukebox::settings::JukeboxSettings>,
    /// Per-track jukebox dial overrides, keyed by Spotify track id.
    #[serde(default)]
    pub jukebox_overrides: HashMap<String, crate::jukebox::settings::PartialJukeboxSettings>,
    /// When true, per-song overrides are ignored and the global settings drive every track.
    #[serde(default)]
    pub jukebox_override_per_song: bool,
    /// Unix seconds of the last successful full fetch of each library list ("tracks",
    /// "albums", "shows"), so a sync re-downloads a list once its cached details are a day old.
    #[serde(default)]
    pub library_full_fetch: HashMap<String, i64>,
}

impl Default for UserState {
    fn default() -> Self {
        Self {
            volume: u16::MAX,
            shuffle: false,
            repeat: queue::RepeatSetting::None,
            queuestate: QueueState::default(),
            playlist_orders: HashMap::new(),
            cache_version: 0,
            playback_state: PlaybackState::Default,
            jukebox: None,
            jukebox_overrides: HashMap::new(),
            jukebox_override_per_song: false,
            library_full_fetch: HashMap::new(),
        }
    }
}

/// Configuration files are read/written relative to this directory.
static BASE_PATH: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The complete configuration (state + user configuration) of ncspot.
pub struct Config {
    /// The configuration file path.
    filename: String,
    /// Configuration set by the user, read only.
    values: RwLock<ConfigValues>,
    /// Runtime state which can't be edited by the user, read/write.
    state: RwLock<UserState>,
    /// Personal Web API client ID resolved at startup from the flag, the environment and the
    /// configuration file. [Config::reload] does not change it.
    personal_client_id: RwLock<Option<PersonalClientId>>,
}

impl Config {
    /// Create a default configuration from in-memory defaults, without touching the filesystem.
    #[cfg(test)]
    pub fn new_for_test() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            filename: String::new(),
            values: RwLock::new(ConfigValues::default()),
            state: RwLock::new(UserState::default()),
            personal_client_id: RwLock::new(None),
        })
    }

    /// Like [Config::new_for_test], with `id` as the personal Web API client ID.
    #[cfg(test)]
    pub fn new_for_test_with_client_id(id: &str) -> std::sync::Arc<Self> {
        let cfg = Self::new_for_test();
        *cfg.personal_client_id.write().unwrap() = Some(PersonalClientId {
            id: id.to_string(),
            source: ClientIdSource::Config,
        });
        cfg
    }

    /// Generate the configuration from the user configuration file and the runtime state file.
    /// `filename` can be used to look for a differently named configuration file.
    /// `client_id_flag` is the value of the `--client-id` command line flag, if given.
    pub fn new(filename: Option<String>, client_id_flag: Option<String>) -> Self {
        let filename = filename.unwrap_or(CONFIGURATION_FILE_NAME.to_owned());
        let values = load(&filename).unwrap_or_else(|e| {
            eprint!(
                "There is an error in your configuration file at {}:\n\n{e}",
                user_configuration_directory()
                    .map(|ref mut path| {
                        path.push(CONFIGURATION_FILE_NAME);
                        path.to_string_lossy().to_string()
                    })
                    .expect("configuration directory expected but not found")
            );
            process::exit(1);
        });

        let env_client_id = std::env::var(client_id::CLIENT_ID_ENV_VAR).ok();
        let personal_client_id = client_id::resolve_personal_client_id(
            client_id_flag.as_deref(),
            env_client_id.as_deref(),
            values.client_id.as_deref(),
        )
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        });

        let mut userstate = {
            let path = config_path(USER_STATE_FILE_NAME);
            CBOR.load_or_generate_default(path, || Ok(UserState::default()), true)
                .expect("could not load user state")
        };

        if let Some(shuffle) = values.shuffle {
            userstate.shuffle = shuffle;
        }

        if let Some(repeat) = values.repeat {
            userstate.repeat = repeat;
        }

        if let Some(playback_state) = values.playback_state.clone() {
            userstate.playback_state = playback_state;
        }

        if let Some(over) = values.jukebox.as_ref().and_then(|j| j.override_per_song) {
            userstate.jukebox_override_per_song = over;
        }

        Self {
            filename,
            values: RwLock::new(values),
            state: RwLock::new(userstate),
            personal_client_id: RwLock::new(personal_client_id),
        }
    }

    /// Get the user configuration values.
    pub fn values(&self) -> RwLockReadGuard<'_, ConfigValues> {
        self.values.read().unwrap()
    }

    /// Get the runtime user state values.
    pub fn state(&self) -> RwLockReadGuard<'_, UserState> {
        self.state.read().unwrap()
    }

    /// The personal Web API client ID, if one is configured and still in use.
    pub fn personal_client_id(&self) -> Option<PersonalClientId> {
        self.personal_client_id.read().unwrap().clone()
    }

    /// Stop using the personal Web API client ID, for example after its authorization failed.
    /// All Web API traffic then uses the shared client ID.
    pub fn disable_personal_client_id(&self) {
        *self.personal_client_id.write().unwrap() = None;
    }

    /// Modify the internal user state through a shared reference using a closure.
    pub fn with_state_mut<F>(&self, cb: F)
    where
        F: Fn(&mut UserState),
    {
        let mut state_guard = self.state.write().unwrap();
        cb(&mut state_guard);
    }

    /// Update the version number of the runtime user state. This should be done before saving it to
    /// disk.
    fn update_state_cache_version(&self) {
        self.with_state_mut(|state| state.cache_version = CACHE_VERSION);
    }

    /// Save runtime state to the user configuration directory.
    pub fn save_state(&self) {
        self.update_state_cache_version();

        let path = config_path(USER_STATE_FILE_NAME);
        debug!("saving user state to {}", path.display());
        if let Err(e) = CBOR.write(path, &*self.state()) {
            error!("Could not save user state: {e}");
        }
    }

    /// Create a [Theme] from the user supplied theme in the configuration file.
    pub fn build_theme(&self) -> Theme {
        crate::theme::load(&self.values().theme)
    }

    /// Attempt to reload the configuration from the configuration file.
    ///
    /// This only updates the values stored in memory but doesn't perform any additional actions
    /// like updating active keybindings. The personal Web API client ID is not re-resolved; a
    /// warning is logged if its configuration value changed.
    pub fn reload(&self) -> Result<(), Box<dyn Error>> {
        let cfg = load(&self.filename)?;
        let old_client_id = self.values().client_id.clone();
        let active_source = self.personal_client_id().map(|p| p.source);
        if client_id_change_needs_restart(
            old_client_id.as_deref(),
            cfg.client_id.as_deref(),
            active_source,
        ) {
            warn!("client_id changed in the configuration file; restart ncspot to apply it");
        }
        *self.values.write().unwrap() = cfg;
        Ok(())
    }
}

/// Whether changing `client_id` in the configuration file from `old` to `new` needs a restart to
/// take effect: the value changed, and neither the flag nor the environment variable overrides
/// the configuration file.
fn client_id_change_needs_restart(
    old: Option<&str>,
    new: Option<&str>,
    active_source: Option<ClientIdSource>,
) -> bool {
    fn normalize(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    let overridden = matches!(
        active_source,
        Some(ClientIdSource::Flag | ClientIdSource::Env)
    );
    normalize(old) != normalize(new) && !overridden
}

/// Parse the configuration file with name `filename` at the configuration base path.
fn load(filename: &str) -> Result<ConfigValues, String> {
    let path = config_path(filename);
    TOML.load_or_generate_default(path, || Ok(ConfigValues::default()), false)
}

/// Read only the `proxy` setting from the user's configuration file, without
/// any side effects (missing files are simply ignored). Called very early in
/// `main`, before the configuration is loaded for real, so that proxy
/// environment variables can be exported before any threads are spawned.
fn read_proxy_setting(filename: Option<String>) -> Option<String> {
    let filename = filename.unwrap_or(CONFIGURATION_FILE_NAME.to_owned());
    let contents = std::fs::read_to_string(config_path(&filename)).ok()?;
    let values: ConfigValues = toml::from_str(&contents).ok()?;
    let proxy = values.proxy?.trim().to_owned();
    if proxy.is_empty() { None } else { Some(proxy) }
}

/// Export the configured `proxy` to the environment so that every HTTP client
/// stack used by ncspot and its dependencies picks it up:
///
/// * `ALL_PROXY` is read by the reqwest stacks (OAuth token exchange and cover
///   art downloads); reqwest understands `socks5h://` natively.
/// * `all_proxy` is read by ureq (rspotify Web API). ureq does not know the
///   `socks5h` scheme, but its `socks5` implementation already hands the
///   hostname to the proxy, so the schemes are equivalent there.
///
/// The librespot session reads its proxy straight from the configuration
/// instead (see `Spotify::session_config`).
///
/// # Safety
///
/// `std::env::set_var` is not thread safe; this must be called from the main
/// thread before any other threads are spawned. It is only called from the
/// top of `main`.
pub fn preload_proxy_environment(filename: Option<String>) {
    let Some(proxy) = read_proxy_setting(filename) else {
        return;
    };
    let ureq_proxy = match proxy.split_once("://") {
        Some(("socks5h", rest)) => format!("socks5://{rest}"),
        _ => proxy.clone(),
    };
    log::info!("Setting proxy {proxy}");
    unsafe {
        std::env::set_var("ALL_PROXY", &proxy);
        std::env::set_var("all_proxy", ureq_proxy);
    }
}

/// Returns the plaform app directories for ncspot if they could be determined,
/// or an error otherwise.
pub fn try_proj_dirs() -> Result<AppDirs, String> {
    match *BASE_PATH
        .read()
        .map_err(|_| String::from("Poisoned RWLock"))?
    {
        Some(ref basepath) => Ok(AppDirs {
            cache_dir: basepath.join(".cache"),
            config_dir: basepath.join(".config"),
            data_dir: basepath.join(".local/share"),
            state_dir: basepath.join(".local/state"),
        }),
        None => AppDirs::new(Some("ncspot"), true)
            .ok_or_else(|| String::from("Couldn't determine platform standard directories")),
    }
}

/// Return the path to the current user's configuration directory, or None if it couldn't be found.
/// This function does not guarantee correct permissions or ownership of the directory!
pub fn user_configuration_directory() -> Option<PathBuf> {
    let project_directories = try_proj_dirs().ok()?;
    Some(project_directories.config_dir)
}

/// Return the path to the current user's cache directory, or None if one couldn't be found. This
/// function does not guarantee correct permissions or ownership of the directory!
pub fn user_cache_directory() -> Option<PathBuf> {
    let project_directories = try_proj_dirs().ok()?;
    Some(project_directories.cache_dir)
}

/// Force create the configuration directory at the default project location, removing anything that
/// isn't a directory but has the same name. Return the path to the configuration file inside the
/// directory.
///
/// This doesn't create the file, only the containing directory.
pub fn config_path(file: &str) -> PathBuf {
    let cfg_dir = user_configuration_directory().unwrap();
    if cfg_dir.exists() && !cfg_dir.is_dir() {
        fs::remove_file(&cfg_dir).expect("unable to remove old config file");
    }
    if !cfg_dir.exists() {
        fs::create_dir_all(&cfg_dir).expect("can't create config folder");
    }
    let mut cfg = cfg_dir.to_path_buf();
    cfg.push(file);
    cfg
}

/// Create the cache directory at the default project location, preserving it if it already exists,
/// and return the path to the cache file inside the directory.
///
/// This doesn't create the file, only the containing directory.
pub fn cache_path(file: &str) -> PathBuf {
    let cache_dir = user_cache_directory().unwrap();
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir).expect("can't create cache folder");
    }
    let mut pb = cache_dir.to_path_buf();
    pb.push(file);
    pb
}

/// Set the configuration base path. All configuration files are read/written relative to this path.
pub fn set_configuration_base_path(base_path: Option<PathBuf>) {
    if let Some(basepath) = base_path {
        if !basepath.exists() {
            fs::create_dir_all(&basepath).expect("could not create basepath directory");
        }
        *BASE_PATH.write().unwrap() = Some(basepath);
    }
}

#[cfg(test)]
mod lyrics_cfg_tests {
    #[test]
    fn lyrics_provider_order_defaults_and_parses() {
        let c = crate::config::LyricsConfig::default();
        assert_eq!(
            c.provider_order(),
            vec![
                crate::lyrics::model::ProviderId::Lrclib,
                crate::lyrics::model::ProviderId::Spotify,
                crate::lyrics::model::ProviderId::Netease,
                crate::lyrics::model::ProviderId::Musixmatch,
            ]
        );
        let c2 = crate::config::LyricsConfig {
            providers: Some(vec!["netease".into(), "lrclib".into(), "bogus".into()]),
            ..Default::default()
        };
        assert_eq!(
            c2.provider_order(),
            vec![
                crate::lyrics::model::ProviderId::Netease,
                crate::lyrics::model::ProviderId::Lrclib
            ]
        );
    }

    #[test]
    fn allow_seek_defaults_to_enabled() {
        let c = crate::config::LyricsConfig::default();
        assert!(c.allow_seek.unwrap_or(true));
    }

    #[test]
    fn jukebox_analysis_source_order_defaults_and_overrides() {
        let c = crate::config::JukeboxConfig::default();
        assert_eq!(c.analysis_source_order(), vec!["spclient", "eternalbox"]);

        let c2 = crate::config::JukeboxConfig {
            analysis_sources: Some(vec!["spclient".into()]),
            ..Default::default()
        };
        assert_eq!(c2.analysis_source_order(), vec!["spclient"]);
    }

    #[test]
    fn userstate_back_compat_defaults_for_new_jukebox_fields() {
        // A state serialized before these fields existed must load with defaults.
        let mut v = serde_json::to_value(super::UserState::default()).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("jukebox_overrides");
        obj.remove("jukebox_override_per_song");
        let restored: super::UserState = serde_json::from_value(v).unwrap();
        assert!(restored.jukebox_overrides.is_empty());
        assert!(!restored.jukebox_override_per_song);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const PERSONAL_ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn client_id_parses_from_toml() {
        let values: ConfigValues =
            toml::from_str(&format!("client_id = \"{PERSONAL_ID}\"")).expect("valid toml");
        assert_eq!(values.client_id.as_deref(), Some(PERSONAL_ID));
    }

    #[test]
    fn missing_client_id_is_none() {
        let values: ConfigValues = toml::from_str("shuffle = true").expect("valid toml");
        assert_eq!(values.client_id, None);
    }

    #[test]
    fn test_config_has_no_personal_client_id() {
        assert_eq!(Config::new_for_test().personal_client_id(), None);
    }

    #[test]
    fn disable_personal_client_id_clears_it() {
        let cfg = Config::new_for_test_with_client_id(PERSONAL_ID);
        assert_eq!(
            cfg.personal_client_id().map(|p| p.id),
            Some(PERSONAL_ID.to_string())
        );
        cfg.disable_personal_client_id();
        assert_eq!(cfg.personal_client_id(), None);
    }

    #[test]
    fn reload_needs_restart_when_config_client_id_changes() {
        assert!(client_id_change_needs_restart(
            None,
            Some(PERSONAL_ID),
            None
        ));
        assert!(client_id_change_needs_restart(
            Some(PERSONAL_ID),
            None,
            Some(ClientIdSource::Config)
        ));
    }

    #[test]
    fn reload_needs_no_restart_when_unchanged_or_overridden() {
        assert!(!client_id_change_needs_restart(
            Some(PERSONAL_ID),
            Some(PERSONAL_ID),
            Some(ClientIdSource::Config)
        ));
        assert!(!client_id_change_needs_restart(None, Some("  "), None));
        assert!(!client_id_change_needs_restart(
            None,
            Some(PERSONAL_ID),
            Some(ClientIdSource::Flag)
        ));
        assert!(!client_id_change_needs_restart(
            None,
            Some(PERSONAL_ID),
            Some(ClientIdSource::Env)
        ));
    }

    #[test]
    fn notify_rate_limit_parses_and_defaults_to_unset() {
        let values: ConfigValues = toml::from_str("notify_rate_limit = false").expect("valid toml");
        assert_eq!(values.notify_rate_limit, Some(false));
        let values: ConfigValues = toml::from_str("").expect("valid toml");
        assert_eq!(values.notify_rate_limit, None);
    }

    #[test]
    fn statusbar_warning_theme_color_parses() {
        let values: ConfigValues =
            toml::from_str("[theme]\nstatusbar_warning = \"light red\"").expect("valid toml");
        assert_eq!(
            values.theme.and_then(|t| t.statusbar_warning).as_deref(),
            Some("light red")
        );
    }
}
