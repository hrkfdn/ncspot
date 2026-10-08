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
/// Client ID of the shared ncspot app, used for Web API calls when no personal client ID is
/// configured, and always for endpoints that are restricted for newer apps.
pub const DEFAULT_WEB_API_CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

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

/// Name of the token cache file for `client_id` in the cache directory. The shared ncspot
/// client ID keeps the original name so existing users are not asked to log in again.
pub fn token_cache_file_name(client_id: &str) -> String {
    if client_id == DEFAULT_WEB_API_CLIENT_ID {
        String::from("rspotify_token.json")
    } else {
        format!("rspotify_token-{client_id}.json")
    }
}

/// Get a Web API token for `client_id` without user interaction: the cached token if it is
/// still valid and `force` is false, otherwise a token refreshed with the cached refresh token.
///
/// Never starts the browser login, so it is safe to call while the TUI is running.
pub fn refresh_rspotify_token(client_id: &str, force: bool) -> Result<rspotify::Token, String> {
    let path = config::cache_path(&token_cache_file_name(client_id));
    let token = fs::read_to_string(&path)
        .ok()
        .and_then(|json| serde_json::from_str::<rspotify::Token>(&json).ok())
        .ok_or_else(|| String::from("no cached Web API token"))?;

    if !force && !token.is_expired() {
        return Ok(token);
    }

    // Spotify's refresh responses may omit the refresh token, in which case it must be reused,
    // so an empty/missing value here means we don't actually have a usable refresh token.
    let refresh_token = token
        .refresh_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| String::from("the cached Web API token has no refresh token"))?;

    info!("Refreshing the Web API token..");
    let oauth_client = OAuthClientBuilder::new(
        client_id,
        &get_client_redirect_uri(ClientUriSuffix::Ncspot),
        NCSPOT_OAUTH_SCOPES.to_vec(),
    )
    .build()
    .map_err(|e| e.to_string())?;
    let new_token = oauth_client
        .refresh_token(refresh_token)
        .map_err(|e| format!("Failed to refresh token: {e}"))?;
    let mapped = map_token(new_token, Some(refresh_token));
    write_token(&path, &mapped);
    Ok(mapped)
}

/// Get a Web API token for `client_id`: the cached one if still valid, a refreshed one, or a new
/// one from the browser login. The login prints to stdout and waits for the browser, so only
/// call this before the TUI starts, or for the shared client ID (which has always done so).
pub fn get_rspotify_token(client_id: &str) -> Result<rspotify::Token, String> {
    match refresh_rspotify_token(client_id, false) {
        Ok(token) => Ok(token),
        Err(e) => {
            info!("No usable Web API token ({e}), starting the OAuth2 login");
            let t = create_rspotify_token(client_id)?;
            write_token(&config::cache_path(&token_cache_file_name(client_id)), &t);
            Ok(t)
        }
    }
}

pub fn create_rspotify_token(client_id: &str) -> Result<rspotify::Token, String> {
    if client_id == DEFAULT_WEB_API_CLIENT_ID {
        println!(
            "To fully enable Web API features, you need to perform a second OAuth2 authorization\n"
        );
    } else {
        println!(
            "To use your personal Web API client ID, you need to perform one more OAuth2 \
             authorization\n"
        );
    }

    let client_builder = OAuthClientBuilder::new(
        client_id,
        &get_client_redirect_uri(ClientUriSuffix::Ncspot),
        NCSPOT_OAUTH_SCOPES.to_vec(),
    );
    let oauth_client = client_builder.build().map_err(|e| e.to_string())?;

    oauth_client
        .get_access_token()
        .map(|token| map_token(token, None))
        .map_err(|e| e.to_string())
}

/// Shown when the logged-in account may not use the personal client ID's app.
const NOT_ALLOWED_MESSAGE: &str = "this Spotify account may not use the app of the personal \
     client ID; add it under User Management at https://developer.spotify.com/dashboard";

/// Result of checking a Web API token with `GET /me`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenCheck {
    /// The token works.
    Valid,
    /// Spotify rejected the token (HTTP 401): it was revoked, or the app was deleted.
    Unauthorized,
    /// The account may not use this app (HTTP 403), for example a Development Mode app whose
    /// User Management list doesn't include it.
    Forbidden,
    /// The check itself failed (network error, 429, 5xx). The token may well be fine.
    Inconclusive,
}

/// Map the outcome of `GET /me` (`Err(Some(status))` for an HTTP error status) to a
/// [TokenCheck].
fn classify_token_check(result: Result<(), Option<u16>>) -> TokenCheck {
    match result {
        Ok(()) => TokenCheck::Valid,
        Err(Some(401)) => TokenCheck::Unauthorized,
        Err(Some(403)) => TokenCheck::Forbidden,
        Err(_) => TokenCheck::Inconclusive,
    }
}

/// What to do with the personal client ID's token after a check.
#[derive(Debug, PartialEq, Eq)]
enum PersonalTokenStep {
    Accept,
    LoginAgain,
    Reject(String),
}

/// Decide the next step after `check`. `after_new_login` is true once the browser login has
/// already been run again, so a rejected token is not retried forever.
fn next_personal_token_step(check: TokenCheck, after_new_login: bool) -> PersonalTokenStep {
    match check {
        TokenCheck::Valid | TokenCheck::Inconclusive => PersonalTokenStep::Accept,
        TokenCheck::Forbidden => PersonalTokenStep::Reject(NOT_ALLOWED_MESSAGE.to_string()),
        TokenCheck::Unauthorized if !after_new_login => PersonalTokenStep::LoginAgain,
        TokenCheck::Unauthorized => PersonalTokenStep::Reject(String::from(
            "Spotify rejected the new token of the personal client ID",
        )),
    }
}

/// Check `token` with one `GET /me` request.
fn check_rspotify_token(token: &rspotify::Token) -> TokenCheck {
    use rspotify::prelude::*;

    let api = rspotify::AuthCodeSpotify::from_token(token.clone());
    let result = match api.current_user() {
        Ok(_) => Ok(()),
        Err(rspotify::ClientError::Http(error)) => match error.as_ref() {
            rspotify::http::HttpError::StatusCode(response) => Err(Some(response.status())),
            _ => Err(None),
        },
        Err(_) => Err(None),
    };
    let check = classify_token_check(result);
    if check == TokenCheck::Inconclusive {
        warn!("Could not check the personal client ID's token ({result:?}), using it anyway");
    }
    check
}

/// Make sure the personal client ID has a working Web API token before the TUI starts. Gets
/// the token (running the browser login if there is none), checks it with `GET /me`, and runs
/// the login once more if Spotify rejects a cached token.
///
/// Returns a reason if the personal client ID can't be used; the caller then falls back to the
/// shared client ID.
pub fn ensure_personal_token(client_id: &str) -> Result<(), String> {
    let mut token = get_rspotify_token(client_id)?;
    let mut after_new_login = false;
    loop {
        match next_personal_token_step(check_rspotify_token(&token), after_new_login) {
            PersonalTokenStep::Accept => return Ok(()),
            PersonalTokenStep::Reject(reason) => return Err(reason),
            PersonalTokenStep::LoginAgain => {
                info!(
                    "Spotify rejected the cached token of the personal client ID, logging in again"
                );
                token = create_rspotify_token(client_id)?;
                write_token(
                    &config::cache_path(&token_cache_file_name(client_id)),
                    &token,
                );
                after_new_login = true;
            }
        }
    }
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
    fn client_ids_keep_expected_values() {
        assert_eq!(SPOTIFY_CLIENT_ID, "65b708073fc0480ea92a077233ca87bd");
        assert_eq!(
            DEFAULT_WEB_API_CLIENT_ID,
            "d420a117a32841c2b3474932e49fb54b"
        );
    }

    #[test]
    fn shared_client_id_keeps_the_original_token_file() {
        assert_eq!(
            token_cache_file_name(DEFAULT_WEB_API_CLIENT_ID),
            "rspotify_token.json"
        );
    }

    #[test]
    fn personal_client_id_gets_its_own_token_file() {
        assert_eq!(
            token_cache_file_name("0123456789abcdef0123456789abcdef"),
            "rspotify_token-0123456789abcdef0123456789abcdef.json"
        );
    }

    #[test]
    fn token_check_classifies_get_me_results() {
        assert_eq!(classify_token_check(Ok(())), TokenCheck::Valid);
        assert_eq!(
            classify_token_check(Err(Some(401))),
            TokenCheck::Unauthorized
        );
        assert_eq!(classify_token_check(Err(Some(403))), TokenCheck::Forbidden);
        assert_eq!(
            classify_token_check(Err(Some(429))),
            TokenCheck::Inconclusive
        );
        assert_eq!(
            classify_token_check(Err(Some(500))),
            TokenCheck::Inconclusive
        );
        assert_eq!(classify_token_check(Err(None)), TokenCheck::Inconclusive);
    }

    #[test]
    fn valid_and_inconclusive_checks_accept_the_token() {
        assert_eq!(
            next_personal_token_step(TokenCheck::Valid, false),
            PersonalTokenStep::Accept
        );
        assert_eq!(
            next_personal_token_step(TokenCheck::Inconclusive, false),
            PersonalTokenStep::Accept
        );
        assert_eq!(
            next_personal_token_step(TokenCheck::Valid, true),
            PersonalTokenStep::Accept
        );
    }

    #[test]
    fn first_unauthorized_check_logs_in_again() {
        assert_eq!(
            next_personal_token_step(TokenCheck::Unauthorized, false),
            PersonalTokenStep::LoginAgain
        );
    }

    #[test]
    fn unauthorized_after_a_new_login_rejects() {
        assert!(matches!(
            next_personal_token_step(TokenCheck::Unauthorized, true),
            PersonalTokenStep::Reject(_)
        ));
    }

    #[test]
    fn forbidden_rejects_with_the_user_management_hint() {
        assert_eq!(
            next_personal_token_step(TokenCheck::Forbidden, false),
            PersonalTokenStep::Reject(NOT_ALLOWED_MESSAGE.to_string())
        );
        assert!(NOT_ALLOWED_MESSAGE.contains("User Management"));
    }
}
