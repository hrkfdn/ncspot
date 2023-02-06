#![allow(dead_code)]

use std::fmt::Write;

/// Returns a human readable String of a Duration
///
/// Example: `3h 12m 53s`
pub fn format_duration(d: &std::time::Duration) -> String {
    let mut s = String::new();
    let mut append_unit = |value, unit| {
        if value > 0 {
            let _ = write!(s, "{value}{unit}");
        }
    };

    let seconds = d.as_secs() % 60;
    let minutes = (d.as_secs() / 60) % 60;
    let hours = (d.as_secs() / 60) / 60;

    append_unit(hours, "h ");
    append_unit(minutes, "m ");
    append_unit(seconds, "s ");

    s.trim_end().to_string()
}

pub fn cache_path_for_url(url: String) -> std::path::PathBuf {
    let mut path = crate::config::cache_path("covers");
    path.push(url.split('/').last().unwrap());
    path
}

pub fn download(url: String, path: std::path::PathBuf) -> Result<(), std::io::Error> {
    let mut resp = reqwest::blocking::get(url)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut file = std::fs::File::create(path)?;

    std::io::copy(&mut resp, &mut file)?;
    Ok(())
}
