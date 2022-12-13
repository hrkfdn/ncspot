use cursive::XY;

pub mod album;
pub mod artist;
pub mod browse;
pub mod contextmenu;
pub mod help;
pub mod layout;
pub mod library;
pub mod list;
pub mod listview;
pub mod modal;
pub mod pagination;
pub mod playlist;
pub mod playlists;
pub mod printer;
pub mod queue;
pub mod search;
pub mod search_results;
pub mod show;
pub mod statusbar;
pub mod tabview;

#[cfg(feature = "cover")]
pub mod cover;

/// Convert absolute mouse coordinates to coordinates relative to the view
/// origin.
fn mouse_coordinates_to_view(absolute: XY<usize>, offset: XY<usize>) -> XY<usize> {
    absolute - offset
}
