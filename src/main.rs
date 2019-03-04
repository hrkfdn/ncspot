extern crate crossbeam_channel;
extern crate cursive;
extern crate failure;
extern crate futures;
extern crate librespot;
extern crate rspotify;
extern crate tokio_core;
extern crate unicode_width;

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
use std::sync::Mutex;

use cursive::event::Key;
use cursive::view::ScrollStrategy;
use cursive::views::*;
use cursive::Cursive;

mod config;
mod events;
mod queue;
mod spotify;
mod theme;
mod ui;

use events::{Event, EventManager};
use queue::QueueChange;
use ui::playlist::PlaylistEvent;

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
    std::env::set_var("RUST_LOG", "ncspot=trace");
    std::env::set_var("RUST_BACKTRACE", "full");

    // Things here may cause the process to abort; we must do them before creating curses windows
    // otherwise the error message will not be seen by a user
    let path = match env::var_os("HOME") {
        None => {
            eprintln!("$HOME not set");
            process::exit(1);
        }
        Some(path) => PathBuf::from(format!("{0}/.config/ncspot", path.into_string().unwrap())),
    };

    let cfg: config::Config = {
        let contents = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            eprintln!("Cannot read config file from {}", path.to_str().unwrap());
            eprintln!(
                "Expected a config file with this format:\n{}",
                toml::to_string_pretty(&config::Config::default()).unwrap()
            );
            process::exit(1)
        });
        toml::from_str(&contents).unwrap_or_else(|e| {
            eprintln!("{}", e);
            process::exit(1)
        })
    };

    let logbuf = TextContent::new("Welcome to ncspot\n");
    let logview = TextView::new_with_content(logbuf.clone());
    init_logger(logbuf);

    let mut cursive = Cursive::default();
    let event_manager = EventManager::new(cursive.cb_sink().clone());

    cursive.add_global_callback('q', |s| s.quit());
    cursive.set_theme(theme::default());
    cursive.set_autorefresh(true);

    let queue = Arc::new(Mutex::new(queue::Queue::new(event_manager.clone())));

    let spotify = Arc::new(spotify::Spotify::new(
        event_manager.clone(),
        cfg.username,
        cfg.password,
        config::CLIENT_ID.to_string(),
        queue.clone(),
    ));

    // global player keybindings (play, pause, stop)
    {
        let spotify = spotify.clone();
        cursive.add_global_callback('P', move |_s| {
            spotify.toggleplayback();
        });
    }

    {
        let spotify = spotify.clone();
        cursive.add_global_callback('S', move |_s| {
            spotify.stop();
        });
    }

    let searchscreen = cursive.active_screen();
    let search = ui::search::SearchView::new(spotify.clone(), queue.clone());
    cursive.add_fullscreen_layer(search.view);

    let playlistscreen = cursive.add_active_screen();
    let mut playlists = ui::playlist::PlaylistView::new(queue.clone(), spotify.clone());
    cursive.add_fullscreen_layer(playlists.view.take().unwrap());

    let queuescreen = cursive.add_active_screen();
    let mut queueview = ui::queue::QueueView::new(queue.clone(), spotify.clone());
    cursive.add_fullscreen_layer(queueview.view.take().unwrap());

    let logscreen = cursive.add_active_screen();
    let logview_scroller = ScrollView::new(logview).scroll_strategy(ScrollStrategy::StickToBottom);
    let logpanel = Panel::new(logview_scroller).title("Log");
    cursive.add_fullscreen_layer(logpanel);

    cursive.add_global_callback(Key::F1, move |s| {
        s.set_screen(logscreen);
    });

    {
        let ev = event_manager.clone();
        cursive.add_global_callback(Key::F2, move |s| {
            s.set_screen(queuescreen);
            ev.send(Event::Queue(QueueChange::Show));
        });
    }

    cursive.add_global_callback(Key::F3, move |s| {
        s.set_screen(searchscreen);
    });

    {
        let ev = event_manager.clone();
        cursive.add_global_callback(Key::F4, move |s| {
            s.set_screen(playlistscreen);
            ev.send(Event::Playlist(PlaylistEvent::Refresh));
        });
    }

    // cursive event loop
    while cursive.is_running() {
        cursive.step();
        for event in event_manager.msg_iter() {
            trace!("event received");
            match event {
                Event::Queue(ev) => queueview.handle_ev(&mut cursive, ev),
                Event::Player(state) => spotify.updatestate(state),
                Event::Playlist(event) => playlists.handle_ev(&mut cursive, event),
            }
        }
    }
}
