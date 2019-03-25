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

pub fn config_path() -> PathBuf {
    let dirs = directories::BaseDirs::new().expect("can't determine config path");
    PathBuf::from(format!(
        "{0}/ncspot",
        dirs.config_dir()
            .to_str()
            .expect("can't convert path to string")
    ))
}

pub fn cache_path() -> PathBuf {
    let proj_dirs =
        ProjectDirs::from("org", "affekt", "ncspot").expect("can't determine project paths");
    let cache_dir = proj_dirs.cache_dir();
    if !cache_dir.exists() {
        fs::create_dir(cache_dir).expect("can't create cache folder");
    }
    let mut pb = proj_dirs.cache_dir().to_path_buf();
    pb.push("playlists.db");
    pb
}
