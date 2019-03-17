use std::collections::HashMap;

pub const CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub username: String,
    pub password: String,
    pub keybindings: Option<HashMap<String, String>>,
}
