use crate::spotify::URIType;

use url::{Host, Url};

pub struct SpotifyURL {
    pub id: String,
    pub uri_type: URIType,
}

impl SpotifyURL {
    fn new(id: &str, uri_type: URIType) -> SpotifyURL {
        SpotifyURL {
            id: id.to_string(),
            uri_type,
        }
    }

    /// Get media id and type from open.spotify.com url
    ///
    /// ```
    /// let result = spotify_url::SpotifyURL::from_url("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC").unwrap();
    /// assert_eq!(result.id, "4uLU6hMCjMI75M1A2tKUQC");
    /// assert_eq!(result.uri_type, URIType::Track);
    /// ```
    pub fn from_url(s: &str) -> Option<SpotifyURL> {
        let url = Url::parse(s).ok()?;
        if url.host() != Some(Host::Domain("open.spotify.com")) {
            return None;
        }

        let mut path_segments = url.path_segments()?;

        let entity = path_segments.next()?;

        let uri_type = match entity.to_lowercase().as_str() {
            "album" => Some(URIType::Album),
            "artist" => Some(URIType::Artist),
            "episode" => Some(URIType::Episode),
            "playlist" => Some(URIType::Playlist),
            "show" => Some(URIType::Show),
            "track" => Some(URIType::Track),
            "user" => {
                let _user_id = path_segments.next()?;
                let entity = path_segments.next()?;

                if entity != "playlist" {
                    return None;
                }

                Some(URIType::Playlist)
            }
            _ => None,
        }?;

        let id = path_segments.next()?;

        Some(SpotifyURL::new(id, uri_type))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::SpotifyURL;
    use crate::spotify::URIType;

    #[test]
    fn test_urls() {
        let mut test_cases = HashMap::new();
        test_cases.insert(
            "https://open.spotify.com/playlist/1XFxe8bkTryTODn0lk4CNa?si=FfSpZ6KPQdieClZbwHakOQ",
            SpotifyURL::new("1XFxe8bkTryTODn0lk4CNa", URIType::Playlist),
        );
        test_cases.insert(
            "https://open.spotify.com/track/6fRJg3R90w0juYoCJXxj2d",
            SpotifyURL::new("6fRJg3R90w0juYoCJXxj2d", URIType::Track),
        );
        test_cases.insert(
            "https://open.spotify.com/user/~villainy~/playlist/0OgoSs65CLDPn6AF6tsZVg",
            SpotifyURL::new("0OgoSs65CLDPn6AF6tsZVg", URIType::Playlist),
        );
        test_cases.insert(
            "https://open.spotify.com/show/4MZfJbM2MXzZdPbv6gi5lJ",
            SpotifyURL::new("4MZfJbM2MXzZdPbv6gi5lJ", URIType::Show),
        );
        test_cases.insert(
            "https://open.spotify.com/episode/3QE6rfmjRaeqXSqeWcIWF6",
            SpotifyURL::new("3QE6rfmjRaeqXSqeWcIWF6", URIType::Episode),
        );
        test_cases.insert(
            "https://open.spotify.com/artist/6LEeAFiJF8OuPx747e1wxR",
            SpotifyURL::new("6LEeAFiJF8OuPx747e1wxR", URIType::Artist),
        );

        for case in test_cases {
            let result = SpotifyURL::from_url(case.0).unwrap();
            assert_eq!(result.id, case.1.id);
            assert_eq!(result.uri_type, case.1.uri_type);
        }
    }
}
