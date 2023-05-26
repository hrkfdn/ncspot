use std::io::Write;
use std::{backtrace, fs::File};

use crate::config;

/// Register a custom panic handler to write backtraces to a file called `backtrace.log` inside the
/// user's cache directory.
pub fn register_backtrace_panic_handler() {
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
