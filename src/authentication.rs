use std::net::TcpListener;

use librespot_core::authentication::Credentials as RespotCredentials;
use librespot_core::cache::Cache;
use librespot_oauth::get_access_token;
use log::info;

use crate::config::{self, Config};
use crate::spotify::Spotify;

pub const SPOTIFY_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

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
    get_access_token(
        SPOTIFY_CLIENT_ID,
        &get_client_redirect_uri(),
        OAUTH_SCOPES.to_vec(),
    )
    .map(|token| RespotCredentials::with_access_token(token.access_token))
    .map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthResponse {
    pub credentials: RespotCredentials,
    pub error: Option<String>,
}
