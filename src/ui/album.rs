use std::sync::{Arc, RwLock};

use cursive::Cursive;
use cursive::view::ViewWrapper;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;
use crate::ui::tabbedview::TabbedView;

pub struct AlbumView {
    album: Album,
    tabs: TabbedView,
}

impl AlbumView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, album: &Album) -> Self {
        let mut album = album.clone();

        album.load_all_tracks(queue.get_spotify());

        let tracks = if let Some(t) = album.tracks.as_ref() {
            t.clone()
        } else {
            Vec::new()
        };

        let artists = album
            .artist_ids
            .iter()
            .zip(album.artists.iter())
            .map(|(id, name)| Artist::new(id.clone(), name.clone()))
            .collect();

        let mut tabs = TabbedView::new();
        tabs.add_tab(
            "Tracks",
            ListView::new(
                Arc::new(RwLock::new(tracks)),
                queue.clone(),
                library.clone(),
            ),
        );
        tabs.add_tab(
            "Artists",
            ListView::new(Arc::new(RwLock::new(artists)), queue, library),
        );

        Self { album, tabs }
    }
}

impl ViewWrapper for AlbumView {
    wrap_impl!(self.tabs: TabbedView);
}

impl ViewExt for AlbumView {
    fn title(&self) -> String {
        format!("{} ({})", self.album.title, self.album.year)
    }

    fn title_sub(&self) -> String {
        if let Some(tracks) = &self.album.tracks {
            let duration_secs: u64 = tracks.iter().map(|t| t.duration as u64 / 1000).sum();
            let duration = std::time::Duration::from_secs(duration_secs);
            let duration_str = crate::utils::format_duration(&duration);
            format!("{} tracks, {}", tracks.len(), duration_str)
        } else {
            "".to_string()
        }
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
