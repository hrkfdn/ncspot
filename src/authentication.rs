use std::path::Path;

use cursive::view::Identifiable;
use cursive::views::*;
use cursive::{CbSink, Cursive};

use librespot::core::authentication::Credentials as RespotCredentials;
use librespot::protocol::authentication::AuthenticationType;

pub fn create_credentials(path: &Path) -> Result<RespotCredentials, String> {
    let mut login_cursive = Cursive::default();
    let mut info_buf = TextContent::new("Failed to authenticate\n");
    info_buf.append(format!(
        "Cannot read config file from {}\n",
        path.to_str().unwrap()
    ));
    let info_view = Dialog::around(TextView::new_with_content(info_buf.clone()))
        .button("Login", move |s| {
            let login_view = Dialog::new()
                .title("Login with Spotify username and password")
                .content(
                    ListView::new()
                        .child("username", EditView::new().with_id("spotify_user"))
                        .child(
                            "password",
                            EditView::new().secret().with_id("spotify_password"),
                        ),
                )
                .button("Login", |s| {
                    let username = s
                        .call_on_id("spotify_user", |view: &mut EditView| view.get_content())
                        .unwrap()
                        .to_string();
                    let auth_data = s
                        .call_on_id("spotify_password", |view: &mut EditView| view.get_content())
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
                .button("Quit", |s| s.quit());
            s.pop_layer();
            s.add_layer(login_view);
        })
        .button("Login with Facebook", |s| {
            let urls: std::collections::HashMap<String, String> =
                reqwest::get("https://login2.spotify.com/v1/config")
                    .expect("didn't connect")
                    .json()
                    .expect("didn't parse");
            // not a dialog to let people copy & paste the URL
            let url_notice = TextView::new(format!("Browse to {}", urls.get("login_url").unwrap()));

            let controls = Button::new("Quit", |s| s.quit());

            let login_view = LinearLayout::new(cursive::direction::Orientation::Vertical)
                .child(url_notice)
                .child(controls);
            let url = urls.get("login_url").unwrap();
            webbrowser::open(url).ok();
            crappy_poller(urls.get("credentials_url").unwrap(), &s.cb_sink());
            s.pop_layer();
            s.add_layer(login_view)
        })
        .button("Quit", |s| s.quit());

    login_cursive.add_layer(info_view);
    login_cursive.run();

    login_cursive
        .user_data()
        .cloned()
        .unwrap_or(Err("Didn't obtain any credentials".to_string()))
}

// TODO: better with futures?
fn crappy_poller(url: &str, app_sink: &CbSink) {
    let app_sink = app_sink.clone();
    let url = url.to_string();
    std::thread::spawn(move || {
        let timeout = std::time::Duration::from_secs(5 * 60);
        let start_time = std::time::SystemTime::now();
        while std::time::SystemTime::now()
            .duration_since(start_time)
            .unwrap_or(timeout)
            < timeout
        {
            if let Ok(mut response) = reqwest::get(&url) {
                if response.status() != reqwest::StatusCode::ACCEPTED {
                    let result = match response.status() {
                        reqwest::StatusCode::OK => {
                            let creds = response
                                .json::<AuthResponse>()
                                .expect("Unable to parse")
                                .credentials;
                            Ok(creds)
                        }

                        _ => Err(format!(
                            "Facebook auth failed with code {}: {}",
                            response.status(),
                            response.text().unwrap()
                        )),
                    };
                    app_sink
                        .send(Box::new(|s: &mut Cursive| {
                            s.set_user_data(result);
                            s.quit();
                        }))
                        .unwrap();
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }

        app_sink
            .send(Box::new(|s: &mut Cursive| {
                s.set_user_data::<Result<RespotCredentials, String>>(Err(
                    "Timed out authenticating".to_string(),
                ));
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
