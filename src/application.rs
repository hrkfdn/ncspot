use std::error::Error;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use cursive::traits::Nameable;
use cursive::{Cursive, CursiveRunner};
use log::{error, info, trace, warn};

#[cfg(unix)]
use signal_hook::{consts::SIGHUP, consts::SIGTERM, iterator::Signals};

use crate::command::Command;
use crate::commands::CommandManager;
use crate::config::{Config, PlaybackState};
use crate::events::{Event, EventManager};
use crate::ext_traits::CursiveExt;
use crate::library::Library;
use crate::queue::Queue;
use crate::spotify::{PlayerEvent, Spotify};
use crate::ui::create_cursive;
use crate::{authentication, ui, utils};
use crate::{command, queue, spotify};

#[cfg(feature = "mpris")]
use crate::mpris::MprisManager;

#[cfg(unix)]
use crate::ipc::{self, IpcSocket};

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
        .level(log::LevelFilter::Debug)
        // Set runtime log level for modules
        .level_for("ncspot", log::LevelFilter::Trace)
        // Output to stdout, files, and other Dispatch configurations
        .chain(fern::log_file(filename)?)
        // Apply globally
        .apply()?;
    Ok(())
}

pub type UserData = Rc<UserDataInner>;
pub struct UserDataInner {
    pub cmd: CommandManager,
}

/// The global Tokio runtime for running asynchronous tasks.
pub static ASYNC_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// The representation of an ncspot application.
pub struct Application {
    /// The music queue which controls playback order.
    queue: Arc<Queue>,
    /// Internally shared
    spotify: Spotify,
    /// Internally shared
    event_manager: EventManager,
    /// The ncspot configuration.
    #[cfg_attr(not(feature = "notify"), allow(dead_code))]
    configuration: Arc<Config>,
    /// An IPC implementation using the D-Bus MPRIS protocol, used to control and inspect ncspot.
    #[cfg(unix)]
    ipc: Option<IpcSocket>,
    /// The object to render to the terminal.
    cursive: CursiveRunner<Cursive>,
}

impl Application {
    /// Create a new ncspot application.
    ///
    /// # Arguments
    ///
    /// * `configuration_file_path` - Relative path to the configuration file inside the base path
    /// * `client_id_flag` - Value of the `--client-id` command line flag, if given
    pub fn new(
        configuration_file_path: Option<String>,
        client_id_flag: Option<String>,
    ) -> Result<Self, Box<dyn Error>> {
        // Things here may cause the process to abort; we must do them before creating curses
        // windows otherwise the error message will not be seen by a user

        ASYNC_RUNTIME
            .set(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let configuration = Arc::new(Config::new(configuration_file_path, client_id_flag));
        let credentials = authentication::get_credentials(&configuration)?;
        let theme = configuration.build_theme();

        if let Err(e) =
            authentication::get_rspotify_token(authentication::DEFAULT_WEB_API_CLIENT_ID)
        {
            error!("Failed to get rspotify token: {e}");
        }

        match configuration.personal_client_id() {
            Some(personal) => {
                info!(
                    "Using personal Web API client ID {} from {}",
                    personal.redacted(),
                    personal.source
                );
                if let Err(e) = authentication::ensure_personal_token(&personal.id) {
                    error!(
                        "Can't use the personal Web API client ID, \
                         falling back to the shared one: {e}"
                    );
                    eprintln!(
                        "Can't use the personal Web API client ID ({e}), using the shared one"
                    );
                    configuration.disable_personal_client_id();
                }
            }
            None => info!("Using the default shared Web API client ID"),
        }

        println!("Connecting to Spotify..");

        // DON'T USE STDOUT AFTER THIS CALL!
        let mut cursive = create_cursive().map_err(|error| error.to_string())?;

        cursive.set_theme(theme.clone());

        #[cfg(all(unix, feature = "pancurses_backend"))]
        cursive.add_global_callback(cursive::event::Event::CtrlChar('z'), |_s| unsafe {
            libc::raise(libc::SIGTSTP);
        });

        let event_manager = EventManager::new(cursive.cb_sink().clone());

        let mut spotify =
            spotify::Spotify::new(event_manager.clone(), credentials, configuration.clone())?;

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
        let mpris_manager = MprisManager::new(
            event_manager.clone(),
            queue.clone(),
            library.clone(),
            spotify.clone(),
        );

        #[cfg(feature = "mpris")]
        spotify.set_mpris(mpris_manager.clone());

        // Load the last played track into the player
        let playback_state = configuration.state().playback_state.clone();
        let queue_state = configuration.state().queuestate.clone();

        if let Some(playable) = queue.get_current() {
            spotify.load(
                &playable,
                playback_state == PlaybackState::Playing,
                queue_state.track_progress.as_millis() as u32,
            );
            spotify.update_track();
            match playback_state {
                PlaybackState::Stopped => {
                    spotify.stop();
                }
                PlaybackState::Paused | PlaybackState::Playing | PlaybackState::Default => {
                    spotify.pause();
                }
            }
        }

        #[cfg(unix)]
        let ipc = if let Ok(runtime_directory) = utils::create_runtime_directory() {
            Some(
                ipc::IpcSocket::new(
                    ASYNC_RUNTIME.get().unwrap().handle(),
                    runtime_directory.join("ncspot.sock"),
                    event_manager.clone(),
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            error!("failed to create IPC socket: no suitable user runtime directory found");
            None
        };

        let jukebox_analysis = Arc::new(crate::jukebox::analysis::AnalysisManager::new(
            vec![
                Box::new(crate::jukebox::analysis::SpclientSource {
                    spotify: Arc::new(spotify.clone()),
                }),
                Box::new(crate::jukebox::analysis::EternalboxSource {
                    base_url: configuration
                        .values()
                        .jukebox
                        .clone()
                        .unwrap_or_default()
                        .eternalbox_url
                        .unwrap_or_else(|| "https://eternalbox.dev".to_string()),
                }),
            ],
            crate::jukebox::analysis::AnalysisCache::new(crate::config::cache_path("jukebox")),
        ));
        let jukebox = crate::jukebox::Jukebox::new(
            queue.clone(),
            jukebox_analysis,
            event_manager.clone(),
            configuration.clone(),
        );

        let mut cmd_manager = CommandManager::new(
            spotify.clone(),
            queue.clone(),
            library.clone(),
            configuration.clone(),
            event_manager.clone(),
            jukebox.clone(),
        );

        cmd_manager.register_all();
        cmd_manager.register_keybindings(&mut cursive);

        let pending_count = cmd_manager.pending_count();

        cursive.set_user_data(Rc::new(UserDataInner { cmd: cmd_manager }));

        let search =
            ui::search::SearchView::new(event_manager.clone(), queue.clone(), library.clone());

        let libraryview = ui::library::LibraryView::new(queue.clone(), library.clone());

        let queueview = ui::queue::QueueView::new(queue.clone(), library.clone());

        #[cfg(feature = "cover")]
        let coverview = ui::cover::CoverView::new(queue.clone(), library.clone(), &configuration);

        // Lyrics is not feature-gated; register it unconditionally.
        let lyrics_manager = Arc::new(crate::lyrics::LyricsManager::new(
            vec![
                Box::new(crate::lyrics::providers::lrclib::Lrclib),
                Box::new(crate::lyrics::providers::spotify::SpotifyProvider {
                    spotify: Arc::new(spotify.clone()),
                }),
                Box::new(crate::lyrics::providers::netease::Netease),
                Box::new(crate::lyrics::providers::musixmatch::Musixmatch {
                    token: configuration
                        .values()
                        .lyrics
                        .clone()
                        .unwrap_or_default()
                        .musixmatch_token
                        .unwrap_or_default(),
                }),
            ],
            crate::lyrics::cache::LyricsCache::new(crate::config::cache_path("lyrics")),
        ));
        let lyricsview = ui::lyrics::LyricsView::new(
            queue.clone(),
            configuration.clone(),
            event_manager.clone(),
            lyrics_manager,
        );

        let jukeboxview =
            ui::jukebox::JukeboxView::new(jukebox.clone(), queue.clone(), configuration.clone());

        let status = ui::statusbar::StatusBar::new(
            queue.clone(),
            Arc::clone(&library),
            pending_count.clone(),
        );

        let mut layout = ui::layout::Layout::new(
            status,
            &event_manager,
            theme,
            Arc::clone(&configuration),
            pending_count,
        )
        .screen("search", search.with_name("search"))
        .screen("library", libraryview.with_name("library"))
        .screen("queue", queueview);

        #[cfg(feature = "cover")]
        layout.add_screen("cover", coverview.with_name("cover"));

        layout.add_screen("lyrics", lyricsview.with_name("lyrics"));

        layout.add_screen("jukebox", jukeboxview.with_name("jukebox"));

        // initial screen is library
        let initial_screen = configuration
            .values()
            .initial_screen
            .clone()
            .unwrap_or_else(|| "library".to_string());
        if layout.has_screen(&initial_screen) {
            layout.set_screen(initial_screen);
        } else {
            error!("Invalid screen name: {initial_screen}");
            layout.set_screen("library");
        }

        cursive.add_fullscreen_layer(layout.with_name("main"));

        Ok(Self {
            queue,
            spotify,
            event_manager,
            configuration,
            #[cfg(unix)]
            ipc,
            cursive,
        })
    }

    /// Start the application and run the event loop.
    pub fn run(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        let mut signals =
            Signals::new([SIGTERM, SIGHUP]).expect("could not register signal handler");

        // cursive event loop
        while self.cursive.is_running() {
            self.cursive.step();
            #[cfg(feature = "cover")]
            self.cursive
                .call_on_name("cover", |view: &mut ui::cover::CoverView| {
                    view.render_to_terminal();
                });
            #[cfg(feature = "jukebox-graphics")]
            self.cursive
                .call_on_name("jukebox", |view: &mut ui::jukebox::JukeboxView| {
                    view.render_to_terminal();
                });
            #[cfg(unix)]
            for signal in signals.pending() {
                if signal == SIGTERM || signal == SIGHUP {
                    info!("Caught {signal}, cleaning up and closing");
                    if let Some(data) = self.cursive.user_data::<UserData>().cloned() {
                        data.cmd.handle(&mut self.cursive, Command::Quit);
                    }
                }
            }
            for event in self.event_manager.msg_iter() {
                match event {
                    Event::Player(state) => {
                        trace!("event received: {state:?}");
                        self.spotify.update_status(state.clone());

                        #[cfg(unix)]
                        if let Some(ref ipc) = self.ipc {
                            ipc.publish(&state, self.queue.get_current());
                        }

                        if state == PlayerEvent::FinishedTrack {
                            self.queue.next(false);
                        }
                    }
                    Event::Queue(event) => {
                        self.queue.handle_event(event);
                    }
                    Event::SessionDied => {
                        if self.spotify.start_worker(None).is_err() {
                            let data: UserData = self
                                .cursive
                                .user_data()
                                .cloned()
                                .expect("user data should be set");
                            data.cmd.handle(&mut self.cursive, Command::Quit);
                        };
                    }
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
                    Event::RateLimited {
                        client,
                        retry_after,
                    } => {
                        info!(
                            "{client} client ID is rate limited for {}s",
                            retry_after.as_secs()
                        );
                        #[cfg(feature = "notify")]
                        if self
                            .configuration
                            .values()
                            .notify_rate_limit
                            .unwrap_or(true)
                        {
                            std::thread::spawn(move || {
                                crate::rate_limit::send_rate_limit_notification(client, retry_after)
                            });
                        }
                    }
                    Event::PersonalClientDisabled { reason } => {
                        let message = format!(
                            "Personal client ID needs a new login, using the shared one until \
                             restart: {reason}"
                        );
                        warn!("{message}");
                        self.cursive
                            .on_layout(|_, mut layout| layout.set_result(Err(message)));
                        // Not gated by notify_rate_limit: this is rare and needs action.
                        #[cfg(feature = "notify")]
                        std::thread::spawn(|| {
                            crate::rate_limit::send_warning_notification(
                                "ncspot: personal client ID needs a new login",
                                "Using the shared client ID until ncspot restarts",
                            )
                        });
                    }
                }
            }
        }
        Ok(())
    }
}
