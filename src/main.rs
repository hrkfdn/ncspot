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
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use cursive::views::*;
use cursive::Cursive;

mod config;
mod spotify;
mod theme;
mod ui;

fn init_logger(content: TextContent) {
    let mut builder = env_logger::Builder::from_default_env();
    {
        builder
            .format(move |_, record| {
                let mut buffer = content.clone();
                let line = format!("[{}] {}\n", record.level(), record.args());
                buffer.append(line.clone());

                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(true)
                    .open("ncspot.log")
                    .unwrap();
                if let Err(e) = writeln!(file, "{}", line) {
                    eprintln!("Couldn't write to file: {}", e);
                }
                Ok(())
            })
            .init();
    }
}

fn main() {
    let logbuf = TextContent::new("Welcome to ncspot\n");
    let logview = TextView::new_with_content(logbuf.clone());
    std::env::set_var("RUST_LOG", "ncspot=trace");
    std::env::set_var("RUST_BACKTRACE", "full");

    init_logger(logbuf);

    let mut cursive = Cursive::default();
    cursive.add_global_callback('q', |s| s.quit());
    cursive.set_theme(theme::default());

    let path = match env::var_os("HOME") {
        None => {
            println!("$HOME not set.");
            process::exit(1)
        }
        Some(path) => PathBuf::from(format!("{0}/.config/ncspot", path.into_string().unwrap())),
    };

    let cfg = config::load(path.to_str().unwrap()).expect("could not load configuration file");

    let spotify = Arc::new(spotify::Spotify::new(
        cfg.username,
        cfg.password,
        cfg.client_id,
    ));

    let logpanel = Panel::new(logview).title("Log");
    //cursive.add_fullscreen_layer(logpanel);
    let search = ui::search::SearchView::new(spotify.clone());
    cursive.add_fullscreen_layer(search.view);

    cursive.run();
}
