use std::fs;
use std::net::TcpListener;

use librespot_core::authentication::Credentials as RespotCredentials;
use librespot_core::cache::Cache;
use librespot_oauth::OAuthClientBuilder;
use log::{error, info};

use crate::config::{self, Config};
use crate::spotify::Spotify;

pub const SPOTIFY_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
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

pub fn get_client_redirect_uri() -> String {
    let auth_port = find_free_port().expect("Could not find free port");
    let redirect_url = format!("http://127.0.0.1:{auth_port}/login");
    redirect_url
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
        &get_client_redirect_uri(),
        OAUTH_SCOPES.to_vec(),
    );
    let oauth_client = client_builder.build().map_err(|e| e.to_string())?;

    oauth_client
        .get_access_token()
        .map(|token| RespotCredentials::with_access_token(token.access_token))
        .map_err(|e| e.to_string())
}

pub fn get_rspotify_token() -> Result<rspotify::Token, String> {
    let path = config::cache_path("rspotify_token.json");
    let token = if let Ok(token_json) = fs::read_to_string(&path) {
        serde_json::from_str::<rspotify::Token>(&token_json).ok()
    } else {
        None
    };

    if let Some(t) = token {
        if !t.is_expired() {
            return Ok(t);
        }

        // Token is expired, try to refresh if we have a refresh token
        if let Some(ref refresh_token) = t.refresh_token {
            info!("Access token expired, attempting to refresh..");
            let client_builder = OAuthClientBuilder::new(
                NCSPOT_CLIENT_ID,
                &get_client_redirect_uri(),
                NCSPOT_OAUTH_SCOPES.to_vec(),
            );
            if let Ok(oauth_client) = client_builder.build() {
                match oauth_client.refresh_token(refresh_token) {
                    Ok(new_token) => {
                        let mapped = map_token(new_token);
                        let _ = fs::write(path, serde_json::to_string_pretty(&mapped).unwrap());
                        return Ok(mapped);
                    }
                    Err(e) => {
                        error!("Failed to refresh token: {e}");
                    }
                }
            }
        }
    }

    let t = create_rspotify_token()?;
    let _ = fs::write(path, serde_json::to_string_pretty(&t).unwrap());
    Ok(t)
}

pub fn create_rspotify_token() -> Result<rspotify::Token, String> {
    println!(
        "To fully enable Web API features, you need to perform a second OAuth2 authorization\n"
    );

    let client_builder = OAuthClientBuilder::new(
        NCSPOT_CLIENT_ID,
        &get_client_redirect_uri(),
        NCSPOT_OAUTH_SCOPES.to_vec(),
    );
    let oauth_client = client_builder.build().map_err(|e| e.to_string())?;

    oauth_client
        .get_access_token()
        .map(map_token)
        .map_err(|e| e.to_string())
}

fn map_token(token: librespot_oauth::OAuthToken) -> rspotify::Token {
    let duration = if token.expires_at > std::time::Instant::now() {
        token.expires_at.duration_since(std::time::Instant::now())
    } else {
        std::time::Duration::from_secs(0)
    };
    let expires_in = chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::seconds(0));

    rspotify::Token {
        access_token: token.access_token,
        expires_in,
        scopes: std::collections::HashSet::new(),
        expires_at: Some(chrono::Utc::now() + expires_in),
        refresh_token: Some(token.refresh_token),
    }
}
