//! The list of playlists that live tests and drive runs created (feature `harness`): one URI
//! per line, appended right after each create so the user can find and remove them later.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// `NCSPOT_TEST_PLAYLISTS_FILE`, else `test-playlists.txt` in the user's real cache dir
/// (not the temporary one unit tests get), where the user's cleanup commands read it.
pub fn file_path() -> PathBuf {
    match std::env::var_os("NCSPOT_TEST_PLAYLISTS_FILE") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => crate::config::user_cache_directory()
            .expect("user cache folder")
            .join("test-playlists.txt"),
    }
}

/// Append `uri` as a new line, creating the file when missing.
pub fn append(path: &Path, uri: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    // A file whose last line lacks its newline must not glue two URIs together.
    let needs_break = fs::read(path)?.last().is_some_and(|b| *b != b'\n');
    if needs_break {
        file.write_all(b"\n")?;
    }
    file.write_all(uri.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

/// The recorded URIs in file order (blank lines skipped); empty when the file is missing.
pub fn read(path: &Path) -> io::Result<Vec<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Drop the n-th line (1-based); returns the removed URI, or `None` when `n` is out of range
/// (the file is then left untouched).
pub fn remove(path: &Path, n: usize) -> io::Result<Option<String>> {
    let mut lines = read(path)?;
    if n == 0 || n > lines.len() {
        return Ok(None);
    }
    let removed = lines.remove(n - 1);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(Some(removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ncspot-testpl-{tag}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test-playlists.txt");
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn append_then_read_keeps_order() {
        let path = temp_file("append");
        assert!(read(&path).unwrap().is_empty(), "missing file reads empty");
        append(&path, "spotify:playlist:a").unwrap();
        append(&path, "spotify:playlist:b").unwrap();
        assert_eq!(
            read(&path).unwrap(),
            ["spotify:playlist:a", "spotify:playlist:b"]
        );
    }

    #[test]
    fn append_repairs_a_missing_final_newline() {
        let path = temp_file("newline");
        fs::write(&path, "spotify:playlist:a").unwrap();
        append(&path, "spotify:playlist:b").unwrap();
        assert_eq!(
            read(&path).unwrap(),
            ["spotify:playlist:a", "spotify:playlist:b"]
        );
    }

    #[test]
    fn remove_returns_the_line_and_keeps_the_rest() {
        let path = temp_file("remove");
        append(&path, "spotify:playlist:a").unwrap();
        append(&path, "spotify:playlist:b").unwrap();
        assert_eq!(
            remove(&path, 1).unwrap().as_deref(),
            Some("spotify:playlist:a")
        );
        assert_eq!(read(&path).unwrap(), ["spotify:playlist:b"]);
        assert_eq!(
            remove(&path, 1).unwrap().as_deref(),
            Some("spotify:playlist:b")
        );
        assert!(read(&path).unwrap().is_empty());
    }

    #[test]
    fn remove_out_of_range_is_none_and_changes_nothing() {
        let path = temp_file("range");
        append(&path, "spotify:playlist:a").unwrap();
        assert_eq!(remove(&path, 0).unwrap(), None);
        assert_eq!(remove(&path, 2).unwrap(), None);
        assert_eq!(read(&path).unwrap(), ["spotify:playlist:a"]);
    }
}
