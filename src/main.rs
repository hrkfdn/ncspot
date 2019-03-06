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
use cursive::traits::Identifiable;
use cursive::view::ScrollStrategy;
use cursive::views::*;
use cursive::Cursive;

mod config;
mod events;
mod queue;
mod spotify;
mod theme;
mod track;
mod ui;

use events::{Event, EventManager};
use queue::QueueEvent;
use spotify::PlayerEvent;
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

    let spotify = Arc::new(spotify::Spotify::new(
        event_manager.clone(),
        cfg.username,
        cfg.password,
        config::CLIENT_ID.to_string(),
    ));

    let queue = Arc::new(Mutex::new(queue::Queue::new(
        event_manager.clone(),
        spotify.clone(),
    )));

    // global player keybindings (play, pause, stop)
    {
        let queue = queue.clone();
        cursive.add_global_callback('P', move |_s| {
            queue.lock().expect("could not lock queue").toggleplayback();
        });
    }

    {
        let queue = queue.clone();
        cursive.add_global_callback('S', move |_s| {
            queue.lock().expect("could not lock queue").stop();
        });
    }

    {
        let queue = queue.clone();
        cursive.add_global_callback('>', move |_s| {
            queue.lock().expect("could not lock queue").next();
        });
    }

    let search = ui::search::SearchView::new(spotify.clone(), queue.clone());

    let mut playlists = ui::playlist::PlaylistView::new(queue.clone(), spotify.clone());

    let mut queueview = ui::queue::QueueView::new(queue.clone());

    let logview_scroller = ScrollView::new(logview).scroll_strategy(ScrollStrategy::StickToBottom);
    let logpanel = Panel::new(logview_scroller).title("Log");

    let status = ui::statusbar::StatusBar::new(queue.clone(), spotify.clone());

    let layout = ui::layout::Layout::new(status)
        .view("search", BoxView::with_full_height(search.view))
        .view("playlists", playlists.view.take().unwrap())
        .view("queue", queueview.view.take().unwrap())
        .view("log", logpanel);

    cursive.add_fullscreen_layer(layout.with_id("main"));

    cursive.add_global_callback(Key::F1, move |s| {
        s.call_on_id("main", |v: &mut ui::layout::Layout| {
            v.set_view("log");
        });
    });

    {
        let ev = event_manager.clone();
        cursive.add_global_callback(Key::F2, move |s| {
            s.call_on_id("main", |v: &mut ui::layout::Layout| {
                v.set_view("queue");
            });
            ev.send(Event::Queue(QueueEvent::Show));
        });
    }

    cursive.add_global_callback(Key::F3, move |s| {
        s.call_on_id("main", |v: &mut ui::layout::Layout| {
            v.set_view("search");
        });
    });

    {
        let ev = event_manager.clone();
        cursive.add_global_callback(Key::F4, move |s| {
            s.call_on_id("main", |v: &mut ui::layout::Layout| {
                v.set_view("playlists");
            });
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
                Event::Player(state) => {
                    if state == PlayerEvent::FinishedTrack {
                        queue.lock().expect("could not lock queue").next();
                    }
                    spotify.update_status(state);
                }
                Event::Playlist(event) => playlists.handle_ev(&mut cursive, event),
            }
        }
    }
}
