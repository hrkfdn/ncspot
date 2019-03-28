extern crate clap;
extern crate crossbeam_channel;
#[macro_use]
extern crate cursive;
extern crate directories;
extern crate failure;
extern crate futures;
extern crate librespot;
extern crate rspotify;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_timer;
extern crate unicode_width;

#[cfg(feature = "mpris")]
extern crate dbus;

#[macro_use]
extern crate serde;
extern crate serde_json;
extern crate toml;

#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

extern crate rand;

use std::process;
use std::sync::Arc;
use std::thread;

use clap::{App, Arg};
use cursive::traits::Identifiable;
use cursive::Cursive;

mod commands;
mod config;
mod events;
mod playlists;
mod queue;
mod spotify;
mod theme;
mod track;
mod traits;
mod ui;

#[cfg(feature = "mpris")]
mod mpris;

use commands::CommandManager;
use events::{Event, EventManager};
use playlists::Playlists;
use spotify::PlayerEvent;

fn setup_logging(filename: &str) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] [{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        // Add blanket level filter -
        .level(log::LevelFilter::Trace)
        // - and per-module overrides
        .level_for("librespot", log::LevelFilter::Debug)
        // Output to stdout, files, and other Dispatch configurations
        .chain(fern::log_file(filename)?)
        // Apply globally
        .apply()?;
    Ok(())
}

fn main() {
    let matches = App::new("ncspot")
        .version("0.1.0")
        .author("Henrik Friedrichsen <henrik@affekt.org> and contributors")
        .about("cross-platform ncurses Spotify client")
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .value_name("FILE")
                .help("Enable debug logging to the specified file")
                .takes_value(true),
        )
        .get_matches();

    if let Some(filename) = matches.value_of("debug") {
        setup_logging(filename).expect("can't setup logging");
    }

    // Things here may cause the process to abort; we must do them before creating curses windows
    // otherwise the error message will not be seen by a user
    let path = config::config_path("config.toml");

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

    let theme = theme::load(&cfg);

    let mut cursive = Cursive::default();
    cursive.set_theme(theme.clone());

    let event_manager = EventManager::new(cursive.cb_sink().clone());

    let spotify = Arc::new(spotify::Spotify::new(
        event_manager.clone(),
        cfg.username.clone(),
        cfg.password.clone(),
    ));

    let queue = Arc::new(queue::Queue::new(spotify.clone()));

    #[cfg(feature = "mpris")]
    let mpris_manager = Arc::new(mpris::MprisManager::new(spotify.clone(), queue.clone()));

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

    let mut cmd_manager = CommandManager::new();
    cmd_manager.register_all(spotify.clone(), queue.clone(), playlists.clone());

    let cmd_manager = Arc::new(cmd_manager);
    CommandManager::register_keybindings(
        cmd_manager.clone(),
        &mut cursive,
        cfg.keybindings.clone(),
    );

    let search = ui::search::SearchView::new(spotify.clone(), queue.clone());

    let playlistsview = ui::playlists::PlaylistView::new(&playlists, queue.clone());

    let queueview = ui::queue::QueueView::new(queue.clone(), playlists.clone());

    let status = ui::statusbar::StatusBar::new(queue.clone(), spotify.clone(), &cfg);

    let mut layout = ui::layout::Layout::new(status, &event_manager, theme)
        .view("search", search.with_id("search"), "Search")
        .view("playlists", playlistsview.with_id("playlists"), "Playlists")
        .view("queue", queueview, "Queue");

    // initial view is queue
    layout.set_view("queue");

    cursive.add_global_callback(':', move |s| {
        s.call_on_id("main", |v: &mut ui::layout::Layout| {
            v.enable_cmdline();
        });
    });

    layout.cmdline.set_on_edit(move |s, cmd, _| {
        s.call_on_id("main", |v: &mut ui::layout::Layout| {
            if cmd.is_empty() {
                v.clear_cmdline();
            }
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

    // cursive event loop
    while cursive.is_running() {
        cursive.step();
        for event in event_manager.msg_iter() {
            trace!("event received");
            match event {
                Event::Player(state) => {
                    if state == PlayerEvent::FinishedTrack {
                        queue.next(false);
                    }
                    spotify.update_status(state);

                    #[cfg(feature = "mpris")]
                    mpris_manager.update();
                }
                Event::Command(cmd) => {
                    cmd_manager.handle(&mut cursive, cmd);
                }
            }
        }
    }
}
