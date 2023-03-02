#[macro_use]
extern crate cursive;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

use std::backtrace;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use cursive::event::EventTrigger;
use cursive::traits::Nameable;
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use log::{error, info, trace};

use ncspot::program_arguments;
#[cfg(unix)]
use signal_hook::{consts::SIGHUP, consts::SIGTERM, iterator::Signals};

mod authentication;
mod command;
mod commands;
mod config;
mod events;
mod ext_traits;
mod library;
mod model;
mod queue;
mod serialization;
mod sharing;
mod spotify;
mod spotify_api;
mod spotify_url;
mod spotify_worker;
mod theme;
mod traits;
mod ui;
mod utils;

#[cfg(unix)]
mod ipc;

#[cfg(feature = "mpris")]
mod mpris;

use crate::command::{Command, JumpMode};
use crate::commands::CommandManager;
use crate::config::{cache_path, Config};
use crate::events::{Event, EventManager};
use crate::ext_traits::CursiveExt;
use crate::library::Library;
use crate::spotify::PlayerEvent;
use crate::ui::contextmenu::ContextMenu;

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

fn credentials_prompt(error_message: Option<String>) -> Result<Credentials, String> {
    if let Some(message) = error_message {
        let mut siv = cursive::default();
        let dialog = cursive::views::Dialog::around(cursive::views::TextView::new(format!(
            "Connection error:\n{message}"
        )))
        .button("Ok", |s| s.quit());
        siv.add_layer(dialog);
        siv.run();
    }

    authentication::create_credentials()
}

fn register_backtrace_panic_handler() {
    // During most of the program, Cursive is responsible for drawing to the
    // tty. Since stdout probably doesn't work as expected during a panic, the
    // backtrace is written to a file at $USER_CACHE_DIR/ncspot/backtrace.log.
    std::panic::set_hook(Box::new(|panic_info| {
        // A panic hook will prevent the default panic handler from being
        // called. An unwrap in this part would cause a hard crash of ncspot.
        // Don't unwrap/expect/panic in here!
        if let Ok(backtrace_log) = config::try_proj_dirs() {
            let mut path = backtrace_log.cache_dir;
            path.push("backtrace.log");
            if let Ok(mut file) = File::create(path) {
                writeln!(file, "{}", backtrace::Backtrace::force_capture()).unwrap_or_default();
                writeln!(file, "{panic_info}").unwrap_or_default();
            }
        }
    }));
}

type UserData = Arc<UserDataInner>;
struct UserDataInner {
    pub cmd: CommandManager,
}

lazy_static!(
    /// The global Tokio runtime for running asynchronous tasks.
    static ref ASYNC_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
);

fn main() -> Result<(), String> {
    register_backtrace_panic_handler();

    let matches = program_arguments().get_matches();

    if let Some(filename) = matches.get_one::<String>("debug") {
        setup_logging(filename).expect("can't setup logging");
    }

    if let Some(basepath) = matches.get_one::<String>("basepath") {
        let path = PathBuf::from_str(basepath).expect("invalid path");
        if !path.exists() {
            fs::create_dir_all(&path).expect("could not create basepath directory");
        }
        *config::BASE_PATH.write().unwrap() = Some(path);
    }

    // Things here may cause the process to abort; we must do them before creating curses windows
    // otherwise the error message will not be seen by a user
    let cfg: Arc<crate::config::Config> = Arc::new(Config::new(
        matches
            .get_one::<String>("config")
            .unwrap_or(&"config.toml".to_string()),
    ));
    let mut credentials = {
        let cache = Cache::new(Some(config::cache_path("librespot")), None, None, None)
            .expect("Could not create librespot cache");
        let cached_credentials = cache.credentials();
        match cached_credentials {
            Some(c) => {
                info!("Using cached credentials");
                c
            }
            None => credentials_prompt(None)?,
        }
    };

    while let Err(error) = spotify::Spotify::test_credentials(credentials.clone()) {
        let error_msg = format!("{error}");
        credentials = credentials_prompt(Some(error_msg))?;
    }

    println!("Connecting to Spotify..");

    // DON'T USE STDOUT AFTER THIS CALL!
    let backend = cursive::backends::try_default().map_err(|e| e.to_string())?;
    let buffered_backend = Box::new(cursive_buffered_backend::BufferedBackend::new(backend));

    let mut cursive = cursive::CursiveRunner::new(cursive::Cursive::new(), buffered_backend);
    cursive.set_window_title("ncspot");

    let theme = cfg.build_theme();
    cursive.set_theme(theme.clone());

    let event_manager = EventManager::new(cursive.cb_sink().clone());

    let spotify = spotify::Spotify::new(event_manager.clone(), credentials, cfg.clone());

    let library = Arc::new(Library::new(&event_manager, spotify.clone(), cfg.clone()));

    let queue = Arc::new(queue::Queue::new(
        spotify.clone(),
        cfg.clone(),
        library.clone(),
    ));

    #[cfg(feature = "mpris")]
    let mpris_manager = Arc::new(mpris::MprisManager::new(
        event_manager.clone(),
        spotify.clone(),
        queue.clone(),
        library.clone(),
    ));

    let mut cmd_manager = CommandManager::new(
        spotify.clone(),
        queue.clone(),
        library.clone(),
        cfg.clone(),
        event_manager.clone(),
    );

    cmd_manager.register_all();
    cmd_manager.register_keybindings(&mut cursive);

    let user_data: UserData = Arc::new(UserDataInner { cmd: cmd_manager });
    cursive.set_user_data(user_data);

    let search = ui::search::SearchView::new(event_manager.clone(), queue.clone(), library.clone());

    let libraryview = ui::library::LibraryView::new(queue.clone(), library.clone());

    let queueview = ui::queue::QueueView::new(queue.clone(), library.clone());

    #[cfg(feature = "cover")]
    let coverview = ui::cover::CoverView::new(queue.clone(), library.clone(), &cfg);

    let status = ui::statusbar::StatusBar::new(queue.clone(), library);

    let mut layout = ui::layout::Layout::new(status, &event_manager, theme)
        .screen("search", search.with_name("search"))
        .screen("library", libraryview.with_name("library"))
        .screen("queue", queueview);

    #[cfg(feature = "cover")]
    layout.add_screen("cover", coverview.with_name("cover"));

    // initial screen is library
    let initial_screen = cfg
        .values()
        .initial_screen
        .clone()
        .unwrap_or_else(|| "library".to_string());
    if layout.has_screen(&initial_screen) {
        layout.set_screen(initial_screen);
    } else {
        error!("Invalid screen name: {}", initial_screen);
        layout.set_screen("library");
    }

    let cmd_key = |cfg: Arc<Config>| cfg.values().command_key.unwrap_or(':');

    {
        let c = cfg.clone();
        cursive.set_on_post_event(
            EventTrigger::from_fn(move |event| {
                event == &cursive::event::Event::Char(cmd_key(c.clone()))
            }),
            move |s| {
                if s.find_name::<ContextMenu>("contextmenu").is_none() {
                    s.call_on_name("main", |v: &mut ui::layout::Layout| {
                        v.enable_cmdline(cmd_key(cfg.clone()));
                    });
                }
            },
        );
    }

    cursive.add_global_callback('/', move |s| {
        if s.find_name::<ContextMenu>("contextmenu").is_none() {
            s.call_on_name("main", |v: &mut ui::layout::Layout| {
                v.enable_jump();
            });
        }
    });

    cursive.add_global_callback(cursive::event::Key::Esc, move |s| {
        if s.find_name::<ContextMenu>("contextmenu").is_none() {
            s.call_on_name("main", |v: &mut ui::layout::Layout| {
                v.clear_cmdline();
            });
        }
    });

    layout.cmdline.set_on_edit(move |s, cmd, _| {
        s.call_on_name("main", |v: &mut ui::layout::Layout| {
            if cmd.is_empty() {
                v.clear_cmdline();
            }
        });
    });

    {
        let ev = event_manager.clone();
        layout.cmdline.set_on_submit(move |s, cmd| {
            s.on_layout(|_, mut layout| layout.clear_cmdline());
            let cmd_without_prefix = &cmd[1..];
            if cmd.strip_prefix('/').is_some() {
                let command = Command::Jump(JumpMode::Query(cmd_without_prefix.to_string()));
                if let Some(data) = s.user_data::<UserData>().cloned() {
                    data.cmd.handle(s, command);
                }
            } else {
                match command::parse(cmd_without_prefix) {
                    Ok(commands) => {
                        if let Some(data) = s.user_data::<UserData>().cloned() {
                            for cmd in commands {
                                data.cmd.handle(s, cmd);
                            }
                        }
                    }
                    Err(err) => {
                        s.on_layout(|_, mut layout| layout.set_result(Err(err.to_string())));
                    }
                }
            }
            ev.trigger();
        });
    }

    cursive.add_fullscreen_layer(layout.with_name("main"));

    #[cfg(all(unix, feature = "pancurses_backend"))]
    cursive.add_global_callback(cursive::event::Event::CtrlChar('z'), |_s| unsafe {
        libc::raise(libc::SIGTSTP);
    });

    #[cfg(unix)]
    let mut signals = Signals::new([SIGTERM, SIGHUP]).expect("could not register signal handler");

    #[cfg(unix)]
    let ipc = {
        ipc::IpcSocket::new(
            ASYNC_RUNTIME.handle(),
            cache_path("ncspot.sock"),
            event_manager.clone(),
        )
        .map_err(|e| e.to_string())?
    };

    // cursive event loop
    while cursive.is_running() {
        cursive.step();
        #[cfg(unix)]
        for signal in signals.pending() {
            if signal == SIGTERM || signal == SIGHUP {
                info!("Caught {}, cleaning up and closing", signal);
                if let Some(data) = cursive.user_data::<UserData>().cloned() {
                    data.cmd.handle(&mut cursive, Command::Quit);
                }
            }
        }
        for event in event_manager.msg_iter() {
            match event {
                Event::Player(state) => {
                    trace!("event received: {:?}", state);
                    spotify.update_status(state.clone());

                    #[cfg(feature = "mpris")]
                    mpris_manager.update();

                    #[cfg(unix)]
                    ipc.publish(&state, queue.get_current());

                    if state == PlayerEvent::FinishedTrack {
                        queue.next(false);
                    }
                }
                Event::Queue(event) => {
                    queue.handle_event(event);
                }
                Event::SessionDied => spotify.start_worker(None),
                Event::IpcInput(input) => match command::parse(&input) {
                    Ok(commands) => {
                        if let Some(data) = cursive.user_data::<UserData>().cloned() {
                            for cmd in commands {
                                info!("Executing command from IPC: {cmd}");
                                data.cmd.handle(&mut cursive, cmd);
                            }
                        }
                    }
                    Err(e) => error!("Parsing error: {e}"),
                },
            }
        }
    }

    Ok(())
}
