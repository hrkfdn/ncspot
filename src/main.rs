extern crate crossbeam_channel;
#[macro_use]
extern crate cursive;
extern crate failure;
extern crate futures;
extern crate librespot;
extern crate rspotify;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_timer;
extern crate unicode_width;
extern crate xdg;

#[cfg(feature = "mpris")]
extern crate dbus;

#[macro_use]
extern crate serde;
extern crate serde_json;
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
use std::thread;

use cursive::traits::{Identifiable};
use cursive::view::{ScrollStrategy};
use cursive::views::*;
use cursive::Cursive;

mod commands;
mod config;
mod events;
mod playlists;
mod queue;
mod spotify;
mod theme;
mod track;
mod ui;
mod traits;

#[cfg(feature = "mpris")]
mod mpris;

use commands::CommandManager;
use events::{Event, EventManager};
use playlists::Playlists;
use spotify::PlayerEvent;

fn init_logger(content: TextContent, write_to_file: bool) {
    let mut builder = env_logger::Builder::from_default_env();
    {
        builder
            .format(move |_, record| {
                let mut buffer = content.clone();
                let line = format!("[{}] {}\n", record.level(), record.args());
                buffer.append(line.clone());

                if write_to_file {
                    let mut file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .append(true)
                        .open("ncspot.log")
                        .unwrap();
                    if let Err(e) = writeln!(file, "{}", line) {
                        eprintln!("Couldn't write to file: {}", e);
                    }
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
    init_logger(logbuf, true);

    let mut cursive = Cursive::default();
    cursive.set_theme(theme::default());

    let event_manager = EventManager::new(cursive.cb_sink().clone());

    let spotify = Arc::new(spotify::Spotify::new(
        event_manager.clone(),
        cfg.username,
        cfg.password,
        config::CLIENT_ID.to_string(),
    ));

    let queue = Arc::new(queue::Queue::new(
        event_manager.clone(),
        spotify.clone(),
    ));

    #[cfg(feature = "mpris")]
    let mpris_manager = Arc::new(mpris::MprisManager::new(spotify.clone(), queue.clone()));

    let search = ui::search::SearchView::new(spotify.clone(), queue.clone());

    let playlists = Arc::new(Playlists::new(&event_manager, &spotify));

    {
        // download playlists via web api in a background thread
        let playlists = playlists.clone();
        thread::spawn(move || {
            // load cache (if existing)
            playlists.load_cache();

            // fetch or update cached playlists
            playlists.fetch_playlists();

            // re-cache for next startup
            playlists.save_cache();
        });
    }

    let playlistsview = ui::playlist::PlaylistView::new(&playlists, queue.clone());

    let queueview = ui::queue::QueueView::new(queue.clone());

    let logview_scroller = ScrollView::new(logview).scroll_strategy(ScrollStrategy::StickToBottom);

    let status = ui::statusbar::StatusBar::new(queue.clone(), spotify.clone());

    let mut layout = ui::layout::Layout::new(status, &event_manager)
        .view("search", search.with_id("search"), "Search")
        .view("log", logview_scroller, "Log")
        .view("playlists", playlistsview, "Playlists")
        .view("queue", queueview, "Queue");

    // initial view is queue
    layout.set_view("queue");

    cursive.add_global_callback(':', move |s| {
        s.call_on_id("main", |v: &mut ui::layout::Layout| {
            v.enable_cmdline();
        });
    });

    {
        let ev = event_manager.clone();
        layout.cmdline.set_on_submit(move |s, cmd| {
            s.call_on_id("main", |v: &mut ui::layout::Layout| {
                v.clear_cmdline();
                ev.send(Event::Command(cmd.to_string()[1..].to_string()));
            });
        });
    }

    cursive.add_fullscreen_layer(layout.with_id("main"));

    let mut cmd_manager = CommandManager::new();
    cmd_manager.register_all(spotify.clone(), queue.clone());

    #[cfg(feature = "mpris")]
    {
        let mpris_manager = mpris_manager.clone();
        cmd_manager.register_callback(Box::new(move || {
            mpris_manager.update();
        }));
    }

    let cmd_manager = Arc::new(cmd_manager);

    CommandManager::register_keybindings(cmd_manager.clone(), &mut cursive, cfg.keybindings);

    // cursive event loop
    while cursive.is_running() {
        cursive.step();
        for event in event_manager.msg_iter() {
            trace!("event received");
            match event {
                Event::Player(state) => {
                    if state == PlayerEvent::FinishedTrack {
                        queue.next();
                    }
                    spotify.update_status(state);

                    #[cfg(feature = "mpris")]
                    mpris_manager.update();
                }
                Event::Playlist(_event) => (),
                Event::Command(cmd) => {
                    cmd_manager.handle(&mut cursive, cmd);
                }
                Event::ScreenChange(_name) => (),
            }
        }
    }
}
