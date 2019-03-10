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
use cursive::traits::{Identifiable, View};
use cursive::view::{ScrollStrategy, Selector};
use cursive::views::*;
use cursive::Cursive;

mod commands;
mod config;
mod events;
mod queue;
mod spotify;
mod theme;
mod track;
mod ui;

use commands::CommandManager;
use events::{Event, EventManager};
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

fn register_keybinding<E: Into<cursive::event::Event>, S: Into<String>>(
    cursive: &mut Cursive,
    ev: &EventManager,
    event: E,
    command: S,
) {
    let ev = ev.clone();
    let cmd = command.into();
    cursive.add_global_callback(event, move |_s| {
        ev.send(Event::Command(cmd.clone()));
    });
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
    init_logger(logbuf, false);

    let mut cursive = Cursive::default();
    cursive.set_theme(theme::default());

    let event_manager = EventManager::new(cursive.cb_sink().clone());
    let mut cmd_manager = CommandManager::new();

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

    let search = ui::search::SearchView::new(spotify.clone(), queue.clone());

    let mut playlists =
        ui::playlist::PlaylistView::new(event_manager.clone(), queue.clone(), spotify.clone());

    let mut queueview = ui::queue::QueueView::new(queue.clone());

    let logview_scroller = ScrollView::new(logview).scroll_strategy(ScrollStrategy::StickToBottom);

    let status = ui::statusbar::StatusBar::new(queue.clone(), spotify.clone());

    let mut layout = ui::layout::Layout::new(status, &event_manager)
        .view("search", search.view.with_id("search"), "Search")
        .view("log", logview_scroller, "Log")
        .view("playlists", playlists.view.take().unwrap(), "Playlists")
        .view("queue", queueview.view.take().unwrap(), "Queue");

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

    // Register commands
    cmd_manager.register(
        "quit",
        vec!["q", "x"],
        Box::new(move |s, _args| {
            s.quit();
            Ok(None)
        }),
    );

    {
        let queue = queue.clone();
        cmd_manager.register(
            "toggleplayback",
            vec!["toggleplay", "toggle", "play", "pause"],
            Box::new(move |_s, _args| {
                queue.lock().expect("could not lock queue").toggleplayback();
                Ok(None)
            }),
        );
    }

    {
        let queue = queue.clone();
        cmd_manager.register(
            "stop",
            Vec::new(),
            Box::new(move |_s, _args| {
                queue.lock().expect("could not lock queue").stop();
                Ok(None)
            }),
        );
    }

    {
        let queue = queue.clone();
        cmd_manager.register(
            "next",
            Vec::new(),
            Box::new(move |_s, _args| {
                queue.lock().expect("could not lock queue").next();
                Ok(None)
            }),
        );
    }

    {
        let queue = queue.clone();
        cmd_manager.register(
            "clear",
            Vec::new(),
            Box::new(move |_s, _args| {
                queue.lock().expect("could not lock queue").clear();
                Ok(None)
            }),
        );
    }

    {
        cmd_manager.register(
            "queue",
            Vec::new(),
            Box::new(move |s, _args| {
                s.call_on_id("main", |v: &mut ui::layout::Layout| {
                    v.set_view("queue");
                });
                Ok(None)
            }),
        );
    }

    {
        cursive.add_global_callback(Key::F1, move |s| {
            s.call_on_id("main", |v: &mut ui::layout::Layout| {
                v.set_view("queue");
            });
        });
    }

    cmd_manager.register(
        "search",
        Vec::new(),
        Box::new(move |s, args| {
            s.call_on_id("main", |v: &mut ui::layout::Layout| {
                v.set_view("search");
            });
            s.call_on_id("search", |v: &mut LinearLayout| {
                v.focus_view(&Selector::Id("search_edit")).unwrap();
            });
            if args.len() >= 1 {
                s.call_on_id("search_edit", |v: &mut EditView| {
                    v.set_content(args.join(" "));
                });
            }
            Ok(None)
        }),
    );

    {
        cmd_manager.register(
            "playlists",
            vec!["lists"],
            Box::new(move |s, _args| {
                s.call_on_id("main", |v: &mut ui::layout::Layout| {
                    v.set_view("playlists");
                });
                Ok(None)
            }),
        );
    }

    cmd_manager.register(
        "log",
        Vec::new(),
        Box::new(move |s, _args| {
            s.call_on_id("main", |v: &mut ui::layout::Layout| {
                v.set_view("log");
            });
            Ok(None)
        }),
    );

    register_keybinding(&mut cursive, &event_manager, 'q', "quit");
    register_keybinding(&mut cursive, &event_manager, 'P', "toggle");
    register_keybinding(&mut cursive, &event_manager, 'S', "stop");
    register_keybinding(&mut cursive, &event_manager, '>', "next");
    register_keybinding(&mut cursive, &event_manager, 'c', "clear");

    register_keybinding(&mut cursive, &event_manager, Key::F1, "queue");
    register_keybinding(&mut cursive, &event_manager, Key::F2, "search");
    register_keybinding(&mut cursive, &event_manager, Key::F3, "playlists");
    register_keybinding(&mut cursive, &event_manager, Key::F9, "log");

    // cursive event loop
    while cursive.is_running() {
        cursive.step();
        for event in event_manager.msg_iter() {
            trace!("event received");
            match event {
                Event::Player(state) => {
                    if state == PlayerEvent::FinishedTrack {
                        queue.lock().expect("could not lock queue").next();
                    }
                    spotify.update_status(state);
                }
                Event::Playlist(event) => playlists.handle_ev(&mut cursive, event),
                Event::Command(cmd) => {
                    // TODO: handle non-error output as well
                    if let Err(e) = cmd_manager.handle(&mut cursive, cmd) {
                        cursive.call_on_id("main", |v: &mut ui::layout::Layout| {
                            v.set_error(e);
                        });
                    }
                }
                Event::ScreenChange(name) => match name.as_ref() {
                    "playlists" => playlists.repopulate(&mut cursive),
                    "queue" => queueview.repopulate(&mut cursive),
                    _ => (),
                },
            }
        }
    }
}
