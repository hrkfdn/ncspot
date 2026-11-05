#[macro_use]
extern crate cursive;
#[macro_use]
extern crate serde;

use std::{path::PathBuf, process::exit};

use application::{Application, setup_logging};
use config::set_configuration_base_path;
use log::error;
use ncspot::program_arguments;

mod activity;
mod application;
mod authentication;
mod cli;
mod client_id;
mod command;
mod commands;
mod config;
mod cover_prefetch;
mod events;
mod ext_traits;
#[cfg(feature = "harness")]
mod harness;
mod internal_api;
mod jukebox;
mod library;
mod library_collection;
mod library_sync;
mod log_redact;
mod lyrics;
mod model;
mod panic;
mod pending_load;
mod playlist_edit;
mod playlist_items;
mod playlist_layout;
mod playlist_writer;
#[cfg(test)]
mod probe_tests;
mod queue;
mod rate_limit;
mod save_log;
mod serialization;
mod session_slot;
mod sharing;
mod spotify;
mod spotify_api;
mod spotify_url;
mod spotify_worker;
#[cfg(any(test, feature = "harness"))]
mod test_playlists;
mod theme;
mod traits;
mod ui;
mod utils;

#[cfg(unix)]
mod ipc;

#[cfg(feature = "mpris")]
mod mpris;

fn main() -> Result<(), String> {
    // Set a custom backtrace hook that writes the backtrace to a file instead of stdout, since
    // stdout is most likely in use by Cursive.
    panic::register_backtrace_panic_handler();

    // Parse the command line arguments.
    let matches = program_arguments().get_matches();

    // Enable debug logging to a file if specified on the command line.
    if let Some(filename) = matches.get_one::<PathBuf>("debug") {
        setup_logging(filename, matches.get_flag("log-secrets"))
            .expect("logger could not be initialized");
    }

    // Set the configuration base path. All configuration files are read/written relative to this
    // path.
    set_configuration_base_path(matches.get_one::<PathBuf>("basepath").cloned());

    match matches.subcommand() {
        Some(("info", _subcommand_matches)) => cli::info(),
        Some((_, _)) => unreachable!(),
        None => {
            // Export proxy environment variables from the configuration file
            // before the tokio runtime and its worker threads are spawned in
            // `Application::new` (set_var is not thread safe).
            config::preload_proxy_environment(matches.get_one::<String>("config").cloned());

            // Create the application.
            let mut application = match Application::new(
                matches.get_one::<String>("config").cloned(),
                matches.get_one::<String>("client-id").cloned(),
            ) {
                Ok(application) => application,
                Err(error) => {
                    eprintln!("{error}");
                    error!("{error}");
                    exit(-1);
                }
            };

            // From here on this thread runs the UI: its Web API calls never wait out a rate
            // limit. Startup work in Application::new above may still wait like a background
            // thread.
            rate_limit::set_ui_thread();

            // Start the application event loop.
            application.run()
        }
    }?;

    Ok(())
}
