use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{fs, process};

use cursive::theme::Theme;
use platform_dirs::AppDirs;

use crate::command::{SortDirection, SortKey};
use crate::playable::Playable;
use crate::queue;
use crate::serialization::{Serializer, CBOR, TOML};

pub const CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ConfigValues {
    pub default_keybindings: Option<bool>,
    pub keybindings: Option<HashMap<String, String>>,
    pub theme: Option<ConfigTheme>,
    pub use_nerdfont: Option<bool>,
    pub audio_cache: Option<bool>,
    pub backend: Option<String>,
    pub backend_device: Option<String>,
    pub volnorm: Option<bool>,
    pub volnorm_pregain: Option<f32>,
    pub notify: Option<bool>,
    pub bitrate: Option<u32>,
    pub album_column: Option<bool>,
    pub gapless: Option<bool>,
    pub shuffle: Option<bool>,
    pub repeat: Option<queue::RepeatSetting>,
    pub cover_max_scale: Option<f32>,
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
}

impl Default for UserState {
    fn default() -> Self {
        UserState {
            volume: u16::max_value(),
            shuffle: false,
            repeat: queue::RepeatSetting::None,
            queuestate: QueueState::default(),
            playlist_orders: HashMap::new(),
        }
    }
}

lazy_static! {
    pub static ref BASE_PATH: RwLock<Option<PathBuf>> = RwLock::new(None);
}

pub struct Config {
    values: RwLock<ConfigValues>,
    state: RwLock<UserState>,
}

impl Config {
    pub fn new() -> Self {
        let values = load().unwrap_or_else(|e| {
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

        Self {
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
        let cfg = load().expect("could not reload config");
        *self.values.write().expect("can't writelock config values") = cfg
    }
}

fn load() -> Result<ConfigValues, String> {
    let path = config_path("config.toml");
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
