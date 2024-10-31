use cursive::{Cursive, CursiveRunner};
use ncspot_common::BIN_NAME;

pub mod album;
pub mod artist;
pub mod browse;
pub mod contextmenu;
pub mod help;
pub mod layout;
pub mod library;
pub mod listview;
pub mod modal;
pub mod pagination;
pub mod playlist;
pub mod playlists;
pub mod queue;
pub mod search;
pub mod search_results;
pub mod show;
pub mod statusbar;
pub mod tabbedview;

#[cfg(feature = "cover")]
pub mod cover;

/// Create a CursiveRunner which implements the drawing logic and event loop.
pub fn create_cursive() -> Result<CursiveRunner<Cursive>, Box<dyn std::error::Error>> {
    let backend = cursive::backends::try_default()?;
    let mut cursive_runner = CursiveRunner::new(cursive::Cursive::new(), backend);

    cursive_runner.set_window_title(BIN_NAME);

    Ok(cursive_runner)
}
