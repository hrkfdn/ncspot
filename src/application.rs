use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use cursive::event::EventTrigger;
use cursive::theme::Theme;
use cursive::traits::Nameable;
use cursive::{Cursive, CursiveRunner};
use log::{error, info, trace};

#[cfg(unix)]
use signal_hook::{consts::SIGHUP, consts::SIGTERM, iterator::Signals};

use crate::command::{Command, JumpMode};
use crate::commands::CommandManager;
use crate::config::{cache_path, Config};
use crate::events::{Event, EventManager};
use crate::ext_traits::CursiveExt;
use crate::ipc::IpcSocket;
use crate::library::Library;
use crate::queue::Queue;
use crate::spotify::{PlayerEvent, Spotify};
use crate::ui::contextmenu::ContextMenu;
use crate::ui::create_cursive;
use crate::{authentication, ui};
use crate::{command, ipc, queue, spotify};

#[cfg(feature = "mpris")]
use crate::mpris::{self, MprisManager};

/// Set up the global logger to log to `filename`.
pub fn setup_logging(filename: &Path) -> Result<(), fern::InitError> {
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

lazy_static!(
    /// The global Tokio runtime for running asynchronous tasks.
    pub static ref ASYNC_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
);

/// The representation of an ncspot application.
pub struct Application {
    /// The Spotify library, which is obtained from the Spotify API using rspotify.
    library: Arc<Library>,
    /// The music queue which controls playback order.
    queue: Arc<Queue>,
    /// Internally shared
    spotify: Spotify,
    /// The configuration provided in the config file.
    configuration: Arc<Config>,
    /// Internally shared
    event_manager: EventManager,
    /// An IPC implementation using the D-Bus MPRIS protocol, used to control and inspect ncspot.
    #[cfg(feature = "mpris")]
    mpris_manager: MprisManager,
    /// An IPC implementation using a Unix domain socket, used to control and inspect ncspot.
    #[cfg(unix)]
    ipc: IpcSocket,
    /// The object to render to the terminal.
    cursive: CursiveRunner<Cursive>,
    /// The theme used to draw the user interface.
    theme: Rc<Theme>,
}

impl Application {
    /// Create a new ncspot application.
    ///
    /// # Arguments
    ///
    /// * `configuration_base_path` - Path to the configuration directory
    /// * `configuration_file_path` - Relative path to the configuration file inside the base path
    pub fn new(configuration_file_path: Option<String>) -> Result<Self, String> {
        // Things here may cause the process to abort; we must do them before creating curses
        // windows otherwise the error message will not be seen by a user

        let configuration = Arc::new(Config::new(configuration_file_path));
        let credentials = authentication::get_credentials(&configuration)?;
        let theme = configuration.build_theme();

        println!("Connecting to Spotify..");

        // DON'T USE STDOUT AFTER THIS CALL!
        let mut cursive = create_cursive().map_err(|error| error.to_string())?;

        cursive.set_theme(theme.clone());

        let event_manager = EventManager::new(cursive.cb_sink().clone());

        let spotify =
            spotify::Spotify::new(event_manager.clone(), credentials, configuration.clone());

        let library = Arc::new(Library::new(
            event_manager.clone(),
            spotify.clone(),
            configuration.clone(),
        ));

        let queue = Arc::new(queue::Queue::new(
            spotify.clone(),
            configuration.clone(),
            library.clone(),
        ));

        #[cfg(feature = "mpris")]
        let mpris_manager = mpris::MprisManager::new(
            event_manager.clone(),
            queue.clone(),
            library.clone(),
            spotify.clone(),
        );

        #[cfg(unix)]
        let ipc = ipc::IpcSocket::new(
            ASYNC_RUNTIME.handle(),
            cache_path("ncspot.sock"),
            event_manager.clone(),
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            library,
            queue,
            spotify,
            configuration,
            event_manager,
            #[cfg(feature = "mpris")]
            mpris_manager,
            #[cfg(unix)]
            ipc,
            cursive,
            theme: Rc::new(theme),
        })
    }

    pub fn run(&mut self) -> Result<(), String> {
        let mut cmd_manager = CommandManager::new(
            self.spotify.clone(),
            self.queue.clone(),
            self.library.clone(),
            self.configuration.clone(),
            self.event_manager.clone(),
        );

        cmd_manager.register_all();
        cmd_manager.register_keybindings(&mut self.cursive);

        let user_data: UserData = Arc::new(UserDataInner { cmd: cmd_manager });
        self.cursive.set_user_data(user_data);

        let search = ui::search::SearchView::new(
            self.event_manager.clone(),
            self.queue.clone(),
            self.library.clone(),
        );

        let libraryview = ui::library::LibraryView::new(self.queue.clone(), self.library.clone());

        let queueview = ui::queue::QueueView::new(self.queue.clone(), self.library.clone());

        #[cfg(feature = "cover")]
        let coverview = ui::cover::CoverView::new(
            self.queue.clone(),
            self.library.clone(),
            &self.configuration,
        );

        let status = ui::statusbar::StatusBar::new(self.queue.clone(), Arc::clone(&self.library));

        let mut layout =
            ui::layout::Layout::new(status, &self.event_manager, Rc::clone(&self.theme))
                .screen("search", search.with_name("search"))
                .screen("library", libraryview.with_name("library"))
                .screen("queue", queueview);

        #[cfg(feature = "cover")]
        layout.add_screen("cover", coverview.with_name("cover"));

        // initial screen is library
        let initial_screen = self
            .configuration
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
            let c = self.configuration.clone();
            let config_clone = Arc::clone(&self.configuration);
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
                        self.mpris_manager.update();

                        #[cfg(unix)]
                        self.ipc.publish(&state, self.queue.get_current());

                        if state == PlayerEvent::FinishedTrack {
                            self.queue.next(false);
                        }
                    }
                    Event::Queue(event) => {
                        self.queue.handle_event(event);
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
