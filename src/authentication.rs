use std::fs;
use std::net::TcpListener;
use std::path::Path;

use librespot_core::authentication::Credentials as RespotCredentials;
use librespot_core::cache::Cache;
use librespot_oauth::OAuthClientBuilder;
use log::{error, info, warn};

use crate::config::{self, Config};
use crate::spotify::Spotify;

pub const SPOTIFY_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

// This is a client ID issued specifically for ncspot tied to my personal Spotify account.
// Please do not use it without my permission.
pub const NCSPOT_CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

static OAUTH_SCOPES: &[&str] = &[
    "playlist-modify",
    "playlist-modify-private",
    "playlist-modify-public",
    "playlist-read",
    "playlist-read-collaborative",
    "playlist-read-private",
    "streaming",
    "user-follow-modify",
    "user-follow-read",
    "user-library-modify",
    "user-library-read",
    "user-modify",
    "user-modify-playback-state",
    "user-modify-private",
    "user-personalized",
    "user-read-currently-playing",
    "user-read-email",
    "user-read-play-history",
    "user-read-playback-position",
    "user-read-playback-state",
    "user-read-private",
    "user-read-recently-played",
    "user-top-read",
];

static NCSPOT_OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-email",
    "user-read-private",
    "user-library-read",
    "user-library-modify",
    "user-read-playback-state",
    "user-modify-playback-state",
    "playlist-read-private",
    "playlist-modify-public",
    "playlist-modify-private",
    "user-follow-read",
    "user-follow-modify",
    "user-top-read",
    "user-read-currently-playing",
    "user-read-recently-played",
];

pub fn find_free_port() -> Result<u16, String> {
    let socket = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    socket
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| e.to_string())
}

enum ClientUriSuffix {
    Ncspot,
    Spotify,
}

fn get_client_redirect_uri(suffix: ClientUriSuffix) -> String {
    let auth_port = find_free_port().expect("Could not find free port");
    match suffix {
        ClientUriSuffix::Ncspot => format!("http://127.0.0.1:{auth_port}/ncspot_login"),
        ClientUriSuffix::Spotify => format!("http://127.0.0.1:{auth_port}/login"),
    }
}

/// Fixed, as development mode apps need the exact registered URI.
pub const OWN_CLIENT_REDIRECT_URI: &str = "http://127.0.0.1:8989/login";

/// The client ID used for Web API calls.
#[derive(Clone, Debug, PartialEq)]
pub enum WebApiClient {
    /// The client ID shared by all ncspot users.
    Shared,
    /// The user's `client_id`.
    Own(String),
}

impl WebApiClient {
    pub fn client_id(&self) -> &str {
        match self {
            Self::Shared => NCSPOT_CLIENT_ID,
            Self::Own(id) => id,
        }
    }

    pub fn token_path(&self) -> std::path::PathBuf {
        config::cache_path(&self.token_file())
    }

    /// Per client ID, so a new `client_id` logs in again.
    fn token_file(&self) -> String {
        match self {
            Self::Shared => "rspotify_token.json".to_string(),
            Self::Own(id) => {
                let id: String = id.chars().filter(char::is_ascii_alphanumeric).collect();
                format!("rspotify_token_{id}.json")
            }
        }
    }

    fn redirect_uri(&self) -> String {
        match self {
            Self::Shared => get_client_redirect_uri(ClientUriSuffix::Ncspot),
            Self::Own(_) => OWN_CLIENT_REDIRECT_URI.to_string(),
        }
    }

    fn oauth_client(&self) -> Result<librespot_oauth::OAuthClient, String> {
        OAuthClientBuilder::new(
            self.client_id(),
            &self.redirect_uri(),
            NCSPOT_OAUTH_SCOPES.to_vec(),
        )
        .build()
        .map_err(|e| e.to_string())
    }
}

/// Get credentials for use with librespot. This first tries to get cached credentials. If no cached
/// credentials are available it will initiate the OAuth2 login process.
pub fn get_credentials(configuration: &Config) -> Result<RespotCredentials, String> {
    let mut credentials = {
        let cache = Cache::new(Some(config::cache_path("librespot")), None, None, None)
            .expect("Could not create librespot cache");
        let cached_credentials = cache.credentials();
        match cached_credentials {
            Some(c) => {
                info!("Using cached credentials");
                c
            }
            None => {
                info!("Attempting to login via OAuth2");
                credentials_prompt(None)?
            }
        }
    };

    while let Err(error) = Spotify::test_credentials(configuration, credentials.clone()) {
        let error_msg = format!("{error}");
        credentials = credentials_prompt(Some(error_msg))?;
    }
    Ok(credentials)
}

fn credentials_prompt(error_message: Option<String>) -> Result<RespotCredentials, String> {
    if let Some(message) = error_message {
        eprintln!("Connection error: {message}");
    }

    create_credentials()
}

pub fn create_credentials() -> Result<RespotCredentials, String> {
    println!("To login you need to perform OAuth2 authorization using your web browser\n");

    let client_builder = OAuthClientBuilder::new(
        SPOTIFY_CLIENT_ID,
        &get_client_redirect_uri(ClientUriSuffix::Spotify),
        OAUTH_SCOPES.to_vec(),
    );
    let oauth_client = client_builder.build().map_err(|e| e.to_string())?;

    oauth_client
        .get_access_token()
        .map(|token| RespotCredentials::with_access_token(token.access_token))
        .map_err(|e| e.to_string())
}

/// The configured `client_id`, unless empty.
pub fn own_client_id(cfg: &Config) -> Option<String> {
    cfg.values()
        .client_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(String::from)
}

/// Remove all cached Web API tokens.
pub fn remove_rspotify_tokens() {
    let Ok(entries) = fs::read_dir(config::cache_path("")) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("rspotify_token")
            && name.ends_with(".json")
            && let Err(e) = fs::remove_file(entry.path())
        {
            error!("Failed to remove {name}: {e}");
        }
    }
}

/// Cached or refreshed token for `client`, else a login if `allow_login` (prints to stdout).
pub fn get_rspotify_token(
    client: &WebApiClient,
    allow_login: bool,
) -> Result<rspotify::Token, String> {
    let path = client.token_path();
    let token = if let Ok(token_json) = fs::read_to_string(&path) {
        serde_json::from_str::<rspotify::Token>(&token_json).ok()
    } else {
        None
    };

    if let Some(t) = token {
        if !t.is_expired() {
            return Ok(t);
        }

        // Token is expired, try to refresh if we have a refresh token. Spotify's refresh
        // responses may omit the refresh token, in which case it must be reused, so an
        // empty/missing value here means we don't actually have a usable refresh token.
        let refresh_token = t.refresh_token.as_deref().filter(|s| !s.is_empty());
        if let Some(refresh_token) = refresh_token {
            info!("Access token expired, attempting to refresh..");
            if let Ok(oauth_client) = client.oauth_client() {
                match oauth_client.refresh_token(refresh_token) {
                    Ok(new_token) => {
                        let mapped = map_token(new_token, Some(refresh_token));
                        write_token(&path, &mapped);
                        return Ok(mapped);
                    }
                    Err(e) => {
                        error!("Failed to refresh token: {e}");
                    }
                }
            }
        }
    }

    if !allow_login {
        return Err(format!(
            "No valid token for {client:?}, restart ncspot to log in again"
        ));
    }

    let t = create_rspotify_token(client)?;
    write_token(&path, &t);
    Ok(t)
}

pub fn create_rspotify_token(client: &WebApiClient) -> Result<rspotify::Token, String> {
    match client {
        WebApiClient::Shared => println!(
            "To fully enable Web API features, you need to perform a second OAuth2 authorization\n"
        ),
        WebApiClient::Own(_) => {
            println!(
                "To use your own client ID, you need to perform another OAuth2 authorization\n"
            )
        }
    }

    client
        .oauth_client()?
        .get_access_token()
        .map(|token| map_token(token, None))
        .map_err(|e| e.to_string())
}

/// Write `token` to `path`, logging (rather than silently discarding) any failure, and
/// restricting file permissions since the file holds a long-lived credential.
fn write_token(path: &Path, token: &rspotify::Token) {
    let json = match serde_json::to_string_pretty(token) {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to serialize rspotify token: {e}");
            return;
        }
    };

    if let Err(e) = fs::write(path, json) {
        error!("Failed to write rspotify token cache: {e}");
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
            warn!("Failed to restrict permissions on rspotify token cache: {e}");
        }
    }
}

/// Map an OAuth token obtained from librespot into the `rspotify::Token` type used for Web
/// API calls.
///
/// Spotify's refresh-token responses may omit `refresh_token` entirely, in which case the
/// previously issued refresh token must be reused (it has not been rotated or invalidated).
/// `librespot_oauth` maps that omission to an empty string, so `fallback_refresh_token` is
/// substituted whenever the token returned by librespot is empty, to avoid ever persisting an
/// unusable refresh token.
fn map_token(
    token: librespot_oauth::OAuthToken,
    fallback_refresh_token: Option<&str>,
) -> rspotify::Token {
    let duration = if token.expires_at > std::time::Instant::now() {
        token.expires_at.duration_since(std::time::Instant::now())
    } else {
        std::time::Duration::from_secs(0)
    };
    let expires_in = chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::seconds(0));

    let refresh_token = if token.refresh_token.is_empty() {
        fallback_refresh_token.map(str::to_string)
    } else {
        Some(token.refresh_token)
    };

    rspotify::Token {
        access_token: token.access_token,
        expires_in,
        scopes: std::collections::HashSet::new(),
        expires_at: Some(chrono::Utc::now() + expires_in),
        refresh_token,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn oauth_token(refresh_token: &str) -> librespot_oauth::OAuthToken {
        librespot_oauth::OAuthToken {
            access_token: "access".to_string(),
            refresh_token: refresh_token.to_string(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(3600),
            token_type: "Bearer".to_string(),
            scopes: Vec::new(),
        }
    }

    #[test]
    fn map_token_keeps_new_refresh_token_when_present() {
        let mapped = map_token(oauth_token("new-refresh-token"), Some("old-refresh-token"));
        assert_eq!(mapped.refresh_token, Some("new-refresh-token".to_string()));
    }

    #[test]
    fn map_token_falls_back_to_previous_refresh_token_when_omitted() {
        let mapped = map_token(oauth_token(""), Some("old-refresh-token"));
        assert_eq!(mapped.refresh_token, Some("old-refresh-token".to_string()));
    }

    #[test]
    fn map_token_yields_no_refresh_token_when_omitted_without_fallback() {
        let mapped = map_token(oauth_token(""), None);
        assert_eq!(mapped.refresh_token, None);
    }

    #[test]
    fn web_api_clients_use_separate_ids_and_token_caches() {
        let own = WebApiClient::Own("my-client-id".to_string());
        assert_eq!(WebApiClient::Shared.client_id(), NCSPOT_CLIENT_ID);
        assert_eq!(own.client_id(), "my-client-id");
        assert_ne!(WebApiClient::Shared.token_file(), own.token_file());
        assert_eq!(
            WebApiClient::Own("../my-client-id".to_string()).token_file(),
            "rspotify_token_myclientid.json"
        );
        assert!(
            WebApiClient::Shared
                .redirect_uri()
                .ends_with("/ncspot_login")
        );
        assert_eq!(own.redirect_uri(), OWN_CLIENT_REDIRECT_URI);
    }
}
