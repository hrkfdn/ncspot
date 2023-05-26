#[macro_use]
extern crate cursive;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

use std::path::PathBuf;

use application::Application;
use ncspot::program_arguments;

mod application;
mod authentication;
mod command;
mod commands;
mod config;
mod events;
mod ext_traits;
mod library;
mod model;
mod panic;
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

fn main() -> Result<(), String> {
    // Set a custom backtrace hook that writes the backtrace to a file instead of stdout, since
    // stdout is most likely in use by Cursive.
    panic::register_backtrace_panic_handler();

    let matches = program_arguments().get_matches();

    let mut application = Application::new(
        matches.get_one::<PathBuf>("debug").cloned(),
        matches.get_one::<PathBuf>("basepath").cloned(),
        matches.get_one::<PathBuf>("config").cloned(),
    )?;

    application.run()
}
