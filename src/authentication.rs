use std::process::Command;

use cursive::traits::Resizable;
use cursive::view::Nameable;
use cursive::views::*;
use cursive::{Cursive, CursiveExt};

use librespot_core::authentication::Credentials as RespotCredentials;
use librespot_protocol::authentication::AuthenticationType;

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
        .button("Quit", Cursive::quit);

    login_cursive.add_layer(info_view);
    login_cursive.run();

    login_cursive
        .user_data()
        .cloned()
        .unwrap_or_else(|| Err("Didn't obtain any credentials".to_string()))
}

pub fn credentials_eval(
    username_cmd: &str,
    password_cmd: &str,
) -> Result<RespotCredentials, String> {
    fn eval(cmd: &str) -> Result<Vec<u8>, String> {
        println!("Executing \"{}\"", cmd);
        let mut result = Command::new("sh")
            .args(["-c", cmd])
            .output()
            .map_err(|e| e.to_string())?
            .stdout;
        if let Some(&last_byte) = result.last() {
            if last_byte == 10 {
                result.pop();
            }
        }

        Ok(result)
    }

    println!("Retrieving username");
    let username = String::from_utf8_lossy(&eval(username_cmd)?).into();
    println!("Retrieving password");
    let password = eval(password_cmd)?;

    Ok(RespotCredentials {
        username,
        auth_type: AuthenticationType::AUTHENTICATION_USER_PASS,
        auth_data: password,
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthResponse {
    pub credentials: RespotCredentials,
    pub error: Option<String>,
}
