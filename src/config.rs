use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

pub const CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
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
    pub statusbar_progress_bg: Option<String>,
    pub statusbar: Option<String>,
    pub statusbar_bg: Option<String>,
    pub cmdline: Option<String>,
    pub cmdline_bg: Option<String>,
}

fn proj_dirs() -> ProjectDirs {
    ProjectDirs::from("org", "affekt", "ncspot").expect("can't determine project paths")
}

pub fn config_path(file: &str) -> PathBuf {
    let proj_dirs = proj_dirs();
    let cfg_dir = proj_dirs.config_dir();
    trace!("{:?}", cfg_dir);
    if cfg_dir.exists() && !cfg_dir.is_dir() {
        fs::remove_file(cfg_dir).expect("unable to remove old config file");
    }
    if !cfg_dir.exists() {
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

/// Configuration and credential file helper
/// Creates a default configuration if none exist, otherwise will optionally overwrite
/// the file if it fails to parse
pub fn load_or_generate_default<
    P: AsRef<Path>,
    T: serde::Serialize + serde::de::DeserializeOwned,
    F: Fn(&Path) -> Result<T, String>,
>(
    path: P,
    default: F,
    default_on_parse_failure: bool,
) -> Result<T, String> {
    let path = path.as_ref();
    // Nothing exists so just write the default and return it
    if !path.exists() {
        let value = default(&path)?;
        return write_content_helper(&path, value);
    }

    // load the serialized content. Always report this failure
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Unable to read {}: {}", path.to_string_lossy(), e))?;

    // Deserialize the content, optionally fall back to default if it fails
    let result = toml::from_str(&contents);
    if default_on_parse_failure && result.is_err() {
        let value = default(&path)?;
        return write_content_helper(&path, value);
    }
    result.map_err(|e| format!("Unable to parse {}: {}", path.to_string_lossy(), e))
}

fn write_content_helper<P: AsRef<Path>, T: serde::Serialize>(
    path: P,
    value: T,
) -> Result<T, String> {
    let content =
        toml::to_string_pretty(&value).map_err(|e| format!("Failed serializing value: {}", e))?;
    fs::write(path.as_ref(), content)
        .map(|_| value)
        .map_err(|e| {
            format!(
                "Failed writing content to {}: {}",
                path.as_ref().display(),
                e
            )
        })
}
