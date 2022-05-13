use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{fs, process};

use cursive::theme::Theme;
use log::{debug, error};
use platform_dirs::AppDirs;

use crate::command::{SortDirection, SortKey};
use crate::model::playable::Playable;
use crate::queue;
use crate::serialization::{Serializer, CBOR, TOML};

pub const CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";
pub const CACHE_VERSION: u16 = 1;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    Default,
}

#[derive(Clone, Serialize, Deserialize, Debug, Hash, strum::EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum LibraryTab {
    Tracks,
    Albums,
    Artists,
    Playlists,
    Podcasts,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum ActiveField {
    Title,
    Artists,
    Album,
    Default,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ActiveFields {
    pub left: Option<Vec<ActiveField>>,
    pub center: Option<Vec<ActiveField>>,
    pub right: Option<bool>,
}
impl ActiveFields {
    fn default() -> Self {
        ActiveFields {
            left: Some(vec![ActiveField::Artists, ActiveField::Title]),
            center: Some(vec![ActiveField::Album]),
            right: Some(true),
        }
    }
}

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
    pub bitrate: Option<u32>,
    pub gapless: Option<bool>,
    pub shuffle: Option<bool>,
    pub repeat: Option<queue::RepeatSetting>,
    pub cover_max_scale: Option<f32>,
    pub playback_state: Option<PlaybackState>,
    pub active_fields: Option<ActiveFields>,
    pub library_tabs: Option<Vec<LibraryTab>>,
}

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
    pub error: Option<String>,
    pub error_bg: Option<String>,
    pub statusbar_progress: Option<String>,
    pub statusbar_progress_bg: Option<String>,
    pub statusbar: Option<String>,
    pub statusbar_bg: Option<String>,
    pub cmdline: Option<String>,
    pub cmdline_bg: Option<String>,
    pub search_match: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SortingOrder {
    pub key: SortKey,
    pub direction: SortDirection,
}

#[derive(Serialize, Default, Deserialize, Debug, Clone)]
pub struct QueueState {
    pub current_track: Option<usize>,
    pub random_order: Option<Vec<usize>>,
    pub track_progress: std::time::Duration,
    pub queue: Vec<Playable>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserState {
    pub volume: u16,
    pub shuffle: bool,
    pub repeat: queue::RepeatSetting,
    pub queuestate: QueueState,
    pub playlist_orders: HashMap<String, SortingOrder>,
    pub cache_version: u16,
    pub playback_state: PlaybackState,
}

impl Default for UserState {
    fn default() -> Self {
        UserState {
            volume: u16::MAX,
            shuffle: false,
            repeat: queue::RepeatSetting::None,
            queuestate: QueueState::default(),
            playlist_orders: HashMap::new(),
            cache_version: 0,
            playback_state: PlaybackState::Default,
        }
    }
}

lazy_static! {
    pub static ref BASE_PATH: RwLock<Option<PathBuf>> = RwLock::new(None);
    pub static ref ACTIVE_FIELDS: RwLock<ActiveFields> = RwLock::new(ActiveFields::default());
}

pub struct Config {
    filename: String,
    values: RwLock<ConfigValues>,
    state: RwLock<UserState>,
}

impl Config {
    pub fn new(filename: &str) -> Self {
        let values = load(filename).unwrap_or_else(|e| {
            eprintln!("could not load config: {}", e);
            process::exit(1);
        });

        let mut userstate = {
            let path = config_path("userstate.cbor");
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

        if let Some(active_fields) = values.active_fields.clone() {
            let mut a = ACTIVE_FIELDS.write().unwrap();
            *a = active_fields;
        }

        Self {
            filename: filename.to_string(),
            values: RwLock::new(values),
            state: RwLock::new(userstate),
        }
    }

    pub fn values(&self) -> RwLockReadGuard<ConfigValues> {
        self.values.read().expect("can't readlock config values")
    }

    pub fn state(&self) -> RwLockReadGuard<UserState> {
        self.state.read().expect("can't readlock user state")
    }

    pub fn with_state_mut<F>(&self, cb: F)
    where
        F: Fn(RwLockWriteGuard<UserState>),
    {
        let state_guard = self.state.write().expect("can't writelock user state");
        cb(state_guard);
    }

    pub fn save_state(&self) {
        // update cache version number
        self.with_state_mut(|mut state| state.cache_version = CACHE_VERSION);

        let path = config_path("userstate.cbor");
        debug!("saving user state to {}", path.display());
        if let Err(e) = CBOR.write(path, self.state().clone()) {
            error!("Could not save user state: {}", e);
        }
    }

    pub fn build_theme(&self) -> Theme {
        let theme = &self.values().theme;
        crate::theme::load(theme)
    }

    pub fn reload(&self) {
        let cfg = load(&self.filename).expect("could not reload config");
        *self.values.write().expect("can't writelock config values") = cfg
    }
}

fn load(filename: &str) -> Result<ConfigValues, String> {
    let path = config_path(filename);
    TOML.load_or_generate_default(path, || Ok(ConfigValues::default()), false)
}

fn proj_dirs() -> AppDirs {
    match *BASE_PATH.read().expect("can't readlock BASE_PATH") {
        Some(ref basepath) => AppDirs {
            cache_dir: basepath.join(".cache"),
            config_dir: basepath.join(".config"),
            data_dir: basepath.join(".local/share"),
            state_dir: basepath.join(".local/state"),
        },
        None => AppDirs::new(Some("ncspot"), true).expect("can't determine project paths"),
    }
}

pub fn config_path(file: &str) -> PathBuf {
    let proj_dirs = proj_dirs();
    let cfg_dir = &proj_dirs.config_dir;
    if cfg_dir.exists() && !cfg_dir.is_dir() {
        fs::remove_file(cfg_dir).expect("unable to remove old config file");
    }
    if !cfg_dir.exists() {
        fs::create_dir_all(cfg_dir).expect("can't create config folder");
    }
    let mut cfg = cfg_dir.to_path_buf();
    cfg.push(file);
    cfg
}

pub fn cache_path(file: &str) -> PathBuf {
    let proj_dirs = proj_dirs();
    let cache_dir = &proj_dirs.cache_dir;
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir).expect("can't create cache folder");
    }
    let mut pb = cache_dir.to_path_buf();
    pb.push(file);
    pb
}
