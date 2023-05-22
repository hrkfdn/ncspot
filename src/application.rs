use crate::{command, ipc, mpris, queue, spotify, ASYNC_RUNTIME};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use cursive::event::EventTrigger;
use cursive::traits::Nameable;
use cursive::{Cursive, CursiveRunner};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use log::{error, info, trace};

use ncspot::program_arguments;
#[cfg(unix)]
use signal_hook::{consts::SIGHUP, consts::SIGTERM, iterator::Signals};

use crate::command::{Command, JumpMode};
use crate::commands::CommandManager;
use crate::config::{self, cache_path, Config};
use crate::events::{Event, EventManager};
use crate::ext_traits::CursiveExt;
use crate::library::Library;
use crate::spotify::{PlayerEvent, Spotify};
use crate::ui::contextmenu::ContextMenu;
use crate::{authentication, ui};

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

pub type UserData = Arc<UserDataInner>;
pub struct UserDataInner {
    pub cmd: CommandManager,
}

/// The representation of an ncspot application.
pub struct Application {
    /// The Spotify library, which is obtained from the Spotify API using rspotify.
    library: Arc<Library>,
    /// Internally shared
    spotify: Spotify,
    /// The configuration provided in the config file.
    config: Arc<Config>,
    /// Internally shared
    event_manager: EventManager,
    /// The object to render to the terminal.
    cursive: CursiveRunner<Cursive>,
}

impl Application {
    pub fn new() -> Result<Self, String> {
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
        let config: Arc<crate::config::Config> = Arc::new(Config::new(
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
                None => {
                    info!("Attempting to resolve credentials via username/password commands");
                    let creds = config.values().credentials.clone().unwrap_or_default();

                    match (creds.username_cmd, creds.password_cmd) {
                        (Some(username_cmd), Some(password_cmd)) => {
                            authentication::credentials_eval(&username_cmd, &password_cmd)?
                        }
                        _ => credentials_prompt(None)?,
                    }
                }
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

        let event_manager = EventManager::new(cursive.cb_sink().clone());

        let spotify = spotify::Spotify::new(event_manager.clone(), credentials, config.clone());

        let library = Arc::new(Library::new(
            &event_manager,
            spotify.clone(),
            config.clone(),
        ));

        Ok(Self {
            library,
            spotify,
            config,
            event_manager,
            cursive,
        })
    }

    pub fn run(&mut self) -> Result<(), String> {
        let theme = self.config.build_theme();
        self.cursive.set_theme(theme.clone());

        let queue = Arc::new(queue::Queue::new(
            self.spotify.clone(),
            self.config.clone(),
            self.library.clone(),
        ));

        #[cfg(feature = "mpris")]
        let mpris_manager = mpris::MprisManager::new(
            self.event_manager.clone(),
            queue.clone(),
            self.library.clone(),
            self.spotify.clone(),
        );

        let mut cmd_manager = CommandManager::new(
            self.spotify.clone(),
            queue.clone(),
            self.library.clone(),
            self.config.clone(),
            self.event_manager.clone(),
        );

        cmd_manager.register_all();
        cmd_manager.register_keybindings(&mut self.cursive);

        let user_data: UserData = Arc::new(UserDataInner { cmd: cmd_manager });
        self.cursive.set_user_data(user_data);

        let search = ui::search::SearchView::new(
            self.event_manager.clone(),
            queue.clone(),
            self.library.clone(),
        );

        let libraryview = ui::library::LibraryView::new(queue.clone(), self.library.clone());

        let queueview = ui::queue::QueueView::new(queue.clone(), self.library.clone());

        #[cfg(feature = "cover")]
        let coverview =
            ui::cover::CoverView::new(queue.clone(), self.library.clone(), &self.config);

        let status = ui::statusbar::StatusBar::new(queue.clone(), Arc::clone(&self.library));

        let mut layout = ui::layout::Layout::new(status, &self.event_manager, theme)
            .screen("search", search.with_name("search"))
            .screen("library", libraryview.with_name("library"))
            .screen("queue", queueview);

        #[cfg(feature = "cover")]
        layout.add_screen("cover", coverview.with_name("cover"));

        // initial screen is library
        let initial_screen = self
            .config
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
            let c = self.config.clone();
            let config_clone = Arc::clone(&self.config);
            self.cursive.set_on_post_event(
                EventTrigger::from_fn(move |event| {
                    event == &cursive::event::Event::Char(cmd_key(c.clone()))
                }),
                move |s| {
                    if s.find_name::<ContextMenu>("contextmenu").is_none() {
                        s.call_on_name("main", |v: &mut ui::layout::Layout| {
                            v.enable_cmdline(cmd_key(config_clone.clone()));
                        });
                    }
                },
            );
        }

        self.cursive.add_global_callback('/', move |s| {
            if s.find_name::<ContextMenu>("contextmenu").is_none() {
                s.call_on_name("main", |v: &mut ui::layout::Layout| {
                    v.enable_jump();
                });
            }
        });

        self.cursive
            .add_global_callback(cursive::event::Key::Esc, move |s| {
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
            let ev = self.event_manager.clone();
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

        self.cursive.add_fullscreen_layer(layout.with_name("main"));

        #[cfg(all(unix, feature = "pancurses_backend"))]
        self.cursive
            .add_global_callback(cursive::event::Event::CtrlChar('z'), |_s| unsafe {
                libc::raise(libc::SIGTSTP);
            });

        #[cfg(unix)]
        let mut signals =
            Signals::new([SIGTERM, SIGHUP]).expect("could not register signal handler");

        #[cfg(unix)]
        let ipc = {
            ipc::IpcSocket::new(
                ASYNC_RUNTIME.handle(),
                cache_path("ncspot.sock"),
                self.event_manager.clone(),
            )
            .map_err(|e| e.to_string())?
        };

        // cursive event loop
        while self.cursive.is_running() {
            self.cursive.step();
            #[cfg(unix)]
            for signal in signals.pending() {
                if signal == SIGTERM || signal == SIGHUP {
                    info!("Caught {}, cleaning up and closing", signal);
                    if let Some(data) = self.cursive.user_data::<UserData>().cloned() {
                        data.cmd.handle(&mut self.cursive, Command::Quit);
                    }
                }
            }
            for event in self.event_manager.msg_iter() {
                match event {
                    Event::Player(state) => {
                        trace!("event received: {:?}", state);
                        self.spotify.update_status(state.clone());

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
                    Event::SessionDied => self.spotify.start_worker(None),
                    Event::IpcInput(input) => match command::parse(&input) {
                        Ok(commands) => {
                            if let Some(data) = self.cursive.user_data::<UserData>().cloned() {
                                for cmd in commands {
                                    info!("Executing command from IPC: {cmd}");
                                    data.cmd.handle(&mut self.cursive, cmd);
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
}
