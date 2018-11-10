use std::fs::File;
use std::io::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub username: String,
    pub password: String,
    pub client_id: String,
}

pub fn load(filename: &str) -> Result<Config, toml::de::Error> {
    let mut f = File::open(filename).expect("ncspot configuration file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    toml::from_str(&contents)
}
