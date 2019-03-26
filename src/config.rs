use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;

pub const CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub username: String,
    pub password: String,
    pub keybindings: Option<HashMap<String, String>>,
    pub theme: Option<ConfigTheme>,
    pub use_nerdfont: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ConfigTheme {
    pub background: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub title: Option<String>,
    pub playing: Option<String>,
    pub playing_bg: Option<String>,
    pub highlight: Option<String>,
    pub highlight_bg: Option<String>,
    pub error: Option<String>,
    pub error_bg: Option<String>,
    pub statusbar_progress: Option<String>,
    pub statusbar: Option<String>,
    pub statusbar_bg: Option<String>,
    pub cmdline: Option<String>,
    pub cmdline_bg: Option<String>,
}

fn proj_dirs () -> ProjectDirs {
    ProjectDirs::from("org", "affekt", "ncspot").expect("can't determine project paths")
}

pub fn config_path(file: &str) -> PathBuf {
    let proj_dirs = proj_dirs();
    let cfg_dir = proj_dirs.config_dir();
    trace!("{:?}", cfg_dir);
    if !cfg_dir.exists() || !cfg_dir.is_dir() {
        fs::create_dir(cfg_dir).expect("can't create config folder");
    }
    let mut cfg = cfg_dir.to_path_buf();
    cfg.push(file);
    cfg
}

pub fn cache_path(file: &str) -> PathBuf {
    let proj_dirs = proj_dirs();
    let cache_dir = proj_dirs.cache_dir();
    if !cache_dir.exists() {
        fs::create_dir(cache_dir).expect("can't create cache folder");
    }
    let mut pb = cache_dir.to_path_buf();
    pb.push(file);
    pb
}
