use std::sync::Arc;

use cursive::Cursive;
use cursive::view::ViewWrapper;
use strum::IntoEnumIterator;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::config::LibraryTab;
use crate::library::Library;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::browse::BrowseView;
use crate::ui::listview::ListView;
use crate::ui::playlists::PlaylistsView;
use crate::ui::tabbedview::TabbedView;

pub struct LibraryView {
    tabs: TabbedView,
    display_name: Option<String>,
}

impl LibraryView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let mut tabview = TabbedView::new();
        let selected_tabs = library
            .cfg
            .values()
            .library_tabs
            .clone()
            .unwrap_or_else(|| Vec::from_iter(LibraryTab::iter()));

        for tab in selected_tabs {
            match tab {
                LibraryTab::Tracks => tabview.add_tab(
                    "Tracks",
                    ListView::new(library.tracks.clone(), queue.clone(), library.clone()),
                ),
                LibraryTab::Albums => tabview.add_tab(
                    "Albums",
                    ListView::new(library.albums.clone(), queue.clone(), library.clone()),
                ),
                LibraryTab::Artists => tabview.add_tab(
                    "Artists",
                    ListView::new(library.artists.clone(), queue.clone(), library.clone()),
                ),
                LibraryTab::Playlists => tabview.add_tab(
                    "Playlists",
                    PlaylistsView::new(queue.clone(), library.clone()),
                ),
                LibraryTab::Podcasts => tabview.add_tab(
                    "Podcasts",
                    ListView::new(library.shows.clone(), queue.clone(), library.clone()),
                ),
                LibraryTab::Browse => {
                    tabview.add_tab("Browse", BrowseView::new(queue.clone(), library.clone()))
                }
            }
        }

        Self {
            tabs: tabview,
            display_name: {
                let hide_username = library.cfg.values().hide_display_names.unwrap_or(false);
                if hide_username {
                    None
                } else {
                    library.display_name.clone()
                }
            },
        }
    }
}

impl ViewWrapper for LibraryView {
    wrap_impl!(self.tabs: TabbedView);
}

impl ViewExt for LibraryView {
    fn title(&self) -> String {
        if let Some(name) = &self.display_name {
            format!("Library of {name}")
        } else {
            "Library".to_string()
        }
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
