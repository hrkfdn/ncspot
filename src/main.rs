extern crate crossbeam_channel;
extern crate cursive;
extern crate failure;
extern crate futures;
extern crate librespot;
extern crate rspotify;
extern crate tokio_core;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate toml;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::env;
use std::io;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};

use cursive::views::*;
use cursive::CbFunc;
use cursive::Cursive;

use librespot::core::spotify_id::SpotifyId;

mod config;
mod spotify;
mod theme;

pub trait CbSpotify: Send {
    fn call_box(self: Box<Self>, &mut spotify::Spotify);
}

impl<F: FnOnce(&mut spotify::Spotify) -> () + Send> CbSpotify for F {
    fn call_box(self: Box<Self>, s: &mut spotify::Spotify) {
        (*self)(s)
    }
}

fn main() {
    let loglines = Arc::new(Mutex::new(Vec::new()));
    std::env::set_var("RUST_LOG", "ncspot=trace");
    let mut builder = env_logger::Builder::from_default_env();
    {
        let mut loglines = loglines.clone();
        builder.format(move |buf, record| {
            let mut lines = loglines.lock().unwrap();
            lines.push(format!("[{}] {}", record.level(), record.args()));
            Ok(())
        }).init();
    }

    // let mut cursive = Cursive::default();
    // cursive.add_global_callback('q', |s| s.quit());
    // cursive.set_theme(theme::default());

    let path = match env::var_os("HOME") {
        None => {
            println!("$HOME not set.");
            process::exit(1)
        }
        Some(path) => PathBuf::from(format!("{0}/.config/ncspot", path.into_string().unwrap())),
    };

    let cfg = config::load(path.to_str().unwrap()).expect("could not load configuration file");

    let spotify = spotify::Spotify::new(cfg.username, cfg.password, cfg.client_id);

    // let track = SpotifyId::from_base62("24zYR2ozYbnhhwulk2NLD4").expect("could not load track");

    // spotify.load(track);
    // thread::sleep(time::Duration::new(3, 0));
    // spotify.play();
    // thread::sleep(time::Duration::new(3, 0));
    // spotify.pause();
    // thread::sleep(time::Duration::new(3, 0));
    // spotify.play();

    // thread::sleep(time::Duration::new(8, 0));
    // spotify.load(track);
    // spotify.play();

    let _ = io::stdin().read(&mut [0u8]).unwrap();
    // cursive.run();
}
