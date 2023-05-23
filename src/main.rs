#[macro_use]
extern crate cursive;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

use std::backtrace;
use std::fs::File;
use std::io::Write;
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

/// Register a custom panic handler to write the backtrace to a file since stdout is in use by the
/// Cursive TUI library during most of the application.
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

// Functionality related to the operating system process itself is implemented here.
// Functionality related to ncspot goes into `Application`.
fn main() -> Result<(), String> {
    register_backtrace_panic_handler();

    let matches = program_arguments().get_matches();

    let mut application = Application::new(
        matches.get_one::<PathBuf>("debug").cloned(),
        matches.get_one::<PathBuf>("basepath").cloned(),
        matches.get_one::<PathBuf>("config").cloned(),
    )?;

    application.run()
}
