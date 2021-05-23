use std::{str::FromStr, time::Duration};

use cursive::traits::Boxable;
use cursive::view::Identifiable;
use cursive::views::*;
use cursive::{CbSink, Cursive, CursiveExt};

use librespot_core::authentication::Credentials as RespotCredentials;
use librespot_protocol::authentication::AuthenticationType;

use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use tiny_http::StatusCode;
use url::Url;

pub fn create_credentials() -> Result<RespotCredentials, String> {
    let mut login_cursive = Cursive::default();

    let info_buf = TextContent::new("Please login to Spotify\n");
    let info_view = Dialog::around(TextView::new_with_content(info_buf))
        .button("Login", move |s| {
            let login_view = Dialog::new()
                .title("Spotify login")
                .content(
                    ListView::new()
                        .child(
                            "Username",
                            EditView::new().with_name("spotify_user").fixed_width(18),
                        )
                        .child(
                            "Password",
                            EditView::new()
                                .secret()
                                .with_name("spotify_password")
                                .fixed_width(18),
                        ),
                )
                .button("Login", |s| {
                    let username = s
                        .call_on_name("spotify_user", |view: &mut EditView| view.get_content())
                        .unwrap()
                        .to_string();
                    let auth_data = s
                        .call_on_name("spotify_password", |view: &mut EditView| view.get_content())
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec();
                    s.set_user_data::<Result<RespotCredentials, String>>(Ok(RespotCredentials {
                        username,
                        auth_type: AuthenticationType::AUTHENTICATION_USER_PASS,
                        auth_data,
                    }));
                    s.quit();
                })
                .button("Quit", Cursive::quit);
            s.pop_layer();
            s.add_layer(login_view);
        })
        .button("Login with Facebook", |s| {
            // Prepare a simple listen server on /login
            //    Server redirects to https://open.spotify.com/desktop/auth/success when
            //    code parameter matches code verififer with code challenge method
            //      call https://accounts.spotify.com/api/token
            //      ?grant type=
            //      &client_id=
            //      &redirect_uri=
            //      &code_verifier=code_verifier
            //      &code=code passed to /login
            //    else https://open.spotify.com/desktop/auth/error
            //    After 15min give up
            log::info!("Creating user auth url");
            let client_id = ClientId::new("65b708073fc0480ea92a077233ca87bd".to_owned());
            let redirect_uri = RedirectUrl::new("http://127.0.0.1:4381/login".to_owned()).unwrap();

            let client = BasicClient::new(
                client_id.clone(), // shouldn't use desktop client id - can I use https://developer.spotify.com/documentation/general/guides/app-settings/ ?
                None, // and if so, do I put the secret here? website says "never reveal it publicly!"
                AuthUrl::new("https://accounts.spotify.com/authorize".to_owned()).unwrap(),
                Some(TokenUrl::new("https://accounts.spotify.com/api/token".to_owned()).unwrap()),
            )
            .set_redirect_uri(redirect_uri);

            let (code_challenge, code_verifier) = PkceCodeChallenge::new_random_sha256();

            let builder = client
                .authorize_url(CsrfToken::new_random)
                .set_pkce_challenge(code_challenge);

            let builder_with_scopes = vec![
                "app-remote-control",
                "playlist-modify",
                "playlist-modify-private",
                "playlist-modify-public",
                "playlist-read",
                "playlist-read-collaborative",
                "playlist-read-private",
                "streaming",
                "ugc-image-upload",
                "user-follow-modify",
                "user-follow-read",
                "user-library-modify",
                "user-library-read",
                "user-modify",
                "user-modify-playback-state",
                "user-modify-private",
                "user-personalized",
                "user-read-birthdate",
                "user-read-currently-playing",
                "user-read-email",
                "user-read-play-history",
                "user-read-playback-position",
                "user-read-playback-state",
                "user-read-private",
                "user-read-recently-played",
                "user-top-read",
            ]
            .into_iter()
            .map(str::to_owned)
            .map(Scope::new)
            .fold(builder, |builder, scope| builder.add_scope(scope));

            let (auth_url, csrf_token) = builder_with_scopes.url();

            let mut entry_uri = Url::from_str("https://accounts.spotify.com/login").unwrap();
            entry_uri
                .query_pairs_mut()
                .append_pair("continue", auth_url.as_str())
                .append_pair("method", "facebook")
                .append_pair("utm_source", "ncspot")
                .append_pair("utm_medium", "desktop");

            log::info!("User redirected to: {}", entry_uri);
            log::info!("After login should redirect to: {}", auth_url);
            let url_notice = TextView::new(format!("Browse to {}", entry_uri));
            let controls = Button::new("Quit", Cursive::quit);

            let login_view = LinearLayout::new(cursive::direction::Orientation::Vertical)
                .child(url_notice)
                .child(controls);

            auth_listener("", client_id, client, code_verifier, &s.cb_sink());
            webbrowser::open(auth_url.as_str()).ok();

            s.pop_layer();
            s.add_layer(login_view)

            /*
            // Poll on server
            let urls: std::collections::HashMap<String, String> =
                reqwest::get("https://login2.spotify.com/v1/config")
                    .expect("didn't connect")
                    .json()
                    .expect("didn't parse");
            // not a dialog to let people copy & paste the URL
            let url_notice = TextView::new(format!("Browse to {}", &urls["login_url"]));

            let controls = Button::new("Quit", Cursive::quit);

            let login_view = LinearLayout::new(cursive::direction::Orientation::Vertical)
                .child(url_notice)
                .child(controls);
            let url = &urls["login_url"];
            webbrowser::open(url).ok();
            auth_poller(&urls["credentials_url"], &s.cb_sink());
            s.pop_layer();
            s.add_layer(login_view)
            */
        })
        .button("Quit", Cursive::quit);

    login_cursive.add_layer(info_view);
    login_cursive.run();

    login_cursive
        .user_data()
        .cloned()
        .unwrap_or_else(|| Err("Didn't obtain any credentials".to_string()))
}

fn auth_listener(
    url: &str,
    client_id: ClientId,
    client: BasicClient,
    code_verifier: oauth2::PkceCodeVerifier,
    app_sink: &CbSink,
) {
    let app_sink = app_sink.clone();
    let url = url.to_string();
    std::thread::spawn(move || {
        log::info!("Launching Spotify auth listen server");
        let timeout = std::time::Duration::from_secs(15 * 60);
        // wait for auth code from listener on redirect_uri
        let server = tiny_http::Server::http("0.0.0.0:4381").unwrap();
        log::info!("Waiting for request from Spotify auth");
        let res = server.recv_timeout(timeout);
        log::info!("Got raw result: {:?}", res);

        let request = res.unwrap().unwrap();
        let url = Url::parse("http://0.0.0.0:4381")
            .unwrap()
            .join(request.url())
            .unwrap();

        let (_, authorization_code) = url
            .query_pairs()
            .find(|pair| {
                let &(ref key, _) = pair;
                key == "code"
            })
            .unwrap();
        log::info!("Redirect back got auth code: {:?}", &authorization_code);
        log::info!("Requested token");

        let http_client_wrapper = |request: oauth2::HttpRequest| {
            log::info!("Requesting token with: {}", request.url);
            log::info!("Requesting token with headers: {:?}", request.headers);
            log::info!(
                "Requesting token with body: {}",
                String::from_utf8(request.body.clone()).unwrap()
            );
            let new_req = oauth2::HttpRequest {
                url: Url::from_str(&format!(
                    "{}?{}",
                    &request.url.to_string(),
                    std::str::from_utf8(request.body.as_slice()).unwrap()
                ))
                .unwrap(),
                method: request.method,
                headers: request.headers,
                body: request.body,
            };
            log::info!("Requesting token with: {}", new_req.url);
            http_client(new_req)
        };

        let token_result = client
            .exchange_code(AuthorizationCode::new(authorization_code.to_string()))
            .set_pkce_verifier(code_verifier)
            .add_extra_param("client_id", client_id.to_string())
            .request(http_client_wrapper)
            .unwrap();
        /*
        .unwrap_or_else(|e| {
            let error: Result<RespotCredentials, _> = Err(e.to_string());
            let _ = request.respond(tiny_http::Response::new(
                tiny_http::StatusCode(302),
                vec![tiny_http::Header::from_str(
                    "Location: https://open.spotify.com/desktop/auth/error",
                )
                .unwrap()],
                std::io::empty(),
                None,
                None,
            ));
            app_sink
                .send(Box::new(|s: &mut Cursive| {
                    s.set_user_data(error);
                    s.quit();
                }))
                .unwrap();
            panic!()
        });
        */

        log::info!("Got back token: {:?}", &token_result);

        let credentials: Result<_, String> = Ok(RespotCredentials {
            username: "medwards@walledcity.ca".to_owned(),
            auth_type: AuthenticationType::AUTHENTICATION_STORED_FACEBOOK_CREDENTIALS,
            auth_data: token_result.access_token().secret().clone().into_bytes(),
        });
        log::info!("Making RespotCredentials: {:?}", &credentials);

        app_sink
            .send(Box::new(|s: &mut Cursive| {
                s.set_user_data(credentials);
                s.quit();
            }))
            .unwrap();
    });
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthResponse {
    pub credentials: RespotCredentials,
    pub error: Option<String>,
}
