#![allow(dead_code)]

use std::{fmt::Write, path::PathBuf};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

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

/// Returns a human readable String of milliseconds in the HH:MM:SS format.
pub fn ms_to_hms(duration: u32) -> String {
    let mut formated_time = String::new();

    let total_seconds = duration / 1000;
    let seconds = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3600;

    if hours > 0 {
        formated_time.push_str(&format!("{hours}:{minutes:02}:"));
    } else {
        formated_time.push_str(&format!("{minutes}:"));
    }
    formated_time.push_str(&format!("{seconds:02}"));

    formated_time
}

pub fn cache_path_for_url(url: String) -> std::path::PathBuf {
    let mut path = crate::config::cache_path("covers");
    path.push(url.split('/').next_back().unwrap());
    path
}

pub fn download(url: String, path: std::path::PathBuf) -> Result<(), std::io::Error> {
    let mut resp = reqwest::blocking::get(url).map_err(std::io::Error::other)?;

    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut file = std::fs::File::create(path)?;

    std::io::copy(&mut resp, &mut file)?;
    Ok(())
}

pub fn normalize_search_text(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}

fn normalize_search_text_with_ranges(text: &str) -> (String, Vec<(usize, usize, usize, usize)>) {
    let mut normalized = String::new();
    let mut ranges = Vec::new();

    for (original_start, character) in text.char_indices() {
        let original_end = original_start + character.len_utf8();

        for normalized_character in character
            .to_lowercase()
            .nfd()
            .filter(|c| !is_combining_mark(*c))
        {
            let normalized_start = normalized.len();
            normalized.push(normalized_character);
            ranges.push((
                normalized_start,
                normalized.len(),
                original_start,
                original_end,
            ));
        }
    }

    (normalized, ranges)
}

pub fn normalized_match_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let query = normalize_search_text(query);
    if query.is_empty() {
        return Vec::new();
    }

    let (normalized_text, ranges) = normalize_search_text_with_ranges(text);
    normalized_text
        .match_indices(&query)
        .filter_map(|(match_start, matched)| {
            let match_end = match_start + matched.len();
            let mut matching_ranges = ranges
                .iter()
                .filter(|(start, end, _, _)| *start >= match_start && *end <= match_end);

            let (_, _, original_start, mut original_end) = *matching_ranges.next()?;
            for (_, _, _, range_end) in matching_ranges {
                original_end = *range_end;
            }

            Some((original_start, original_end))
        })
        .collect()
}

/// Create the application specific runtime directory and return the path to it.
///
/// If the directory already exists and has the correct permissions, this function just returns the
/// existing directory. The contents stored in this directory are not necessarily persisted across
/// reboots. Stored files should be small since they could reside in memory (like on a tmpfs mount).
#[cfg(unix)]
pub fn create_runtime_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    use std::{
        fs::{self, Permissions},
        os::unix::prelude::PermissionsExt,
    };

    let user_runtime_directory = user_runtime_directory().ok_or("no runtime directory found")?;

    let creation_result = fs::create_dir(&user_runtime_directory);

    if creation_result.is_ok()
        || matches!(
            creation_result.as_ref().unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        )
    {
        // Needed when created inside a world readable directory, to prevent unauthorized access.
        // Doesn't hurt otherwise.
        fs::set_permissions(&user_runtime_directory, Permissions::from_mode(0o700))?;

        Ok(user_runtime_directory)
    } else {
        #[allow(clippy::unnecessary_unwrap)]
        Err(Box::new(creation_result.unwrap_err()))
    }
}

/// Return the path to the current user's runtime directory, or None if it couldn't be found.
/// This function does not guarantee correct ownership or permissions of the directory.
#[cfg(unix)]
pub fn user_runtime_directory() -> Option<PathBuf> {
    let linux_runtime_directory =
        PathBuf::from(format!("/run/user/{}/", unsafe { libc::getuid() }));
    let unix_runtime_directory = PathBuf::from("/tmp/");

    if let Some(xdg_runtime_directory) = xdg_runtime_directory() {
        Some(xdg_runtime_directory.join("ncspot"))
    } else if cfg!(target_os = "linux") && linux_runtime_directory.exists() {
        Some(linux_runtime_directory.join("ncspot"))
    } else if unix_runtime_directory.exists() {
        Some(unix_runtime_directory.join(format!("ncspot-{}", unsafe { libc::getuid() })))
    } else {
        None
    }
}

#[cfg(unix)]
fn xdg_runtime_directory() -> Option<PathBuf> {
    std::env::var("XDG_RUNTIME_DIR").ok().map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_normalization_ignores_accents_and_case() {
        assert_eq!(
            normalize_search_text("Tití Me Preguntó"),
            "titi me pregunto"
        );
        assert_eq!(normalize_search_text("CAFÉ"), "cafe");
    }

    #[test]
    fn normalized_match_ranges_map_back_to_original_text() {
        let text = "Tití Me Preguntó";
        let ranges = normalized_match_ranges(text, "pregunto");
        let matches: Vec<&str> = ranges
            .iter()
            .map(|(start, end)| &text[*start..*end])
            .collect();

        assert_eq!(matches, vec!["Preguntó"]);
    }
}
