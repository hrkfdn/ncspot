// use std::sync::{Arc, RwLock};

// use cursive::view::ViewWrapper;
// use cursive::Cursive;
// use cursive::views::ScrollView;

// use crate::command::Command;
// use crate::commands::CommandResult;
// use crate::library::Library;
// use crate::model::album::Album;
// use crate::model::artist::Artist;
// use crate::queue::Queue;
// use crate::traits::ViewExt;
// use crate::ui::tabview::TabView;

// use super::list::List;

// pub struct AlbumView {
//     album: Album,
//     tabs: TabView,
// }

// impl AlbumView {
//     pub fn new(queue: Arc<Queue>, library: Arc<Library>, album: &Album) -> Self {
//         let mut album = album.clone();

//         album.load_all_tracks(queue.get_spotify());

//         let tracks = if let Some(t) = album.tracks.as_ref() {
//             t.clone()
//         } else {
//             Vec::new()
//         };

//         let artists = album
//             .artist_ids
//             .iter()
//             .zip(album.artists.iter())
//             .map(|(id, name)| Artist::new(id.clone(), name.clone()))
//             .collect();

//         let tabs = TabView::new()
//             .tab(
//                 "tracks",
//                 ScrollView::new(List::new(
//                     Arc::new(RwLock::new(tracks)),
//                 ))
//             )
//             .tab(
//                 "artists",
//                 ScrollView::new(List::new(Arc::new(RwLock::new(artists)))),
//             );

//         Self { album, tabs }
//     }
// }

// impl ViewWrapper for AlbumView {
//     wrap_impl!(self.tabs: TabView);
// }

// impl ViewExt for AlbumView {
//     fn title(&self) -> String {
//         format!("{} ({})", self.album.title, self.album.year)
//     }

//     fn title_sub(&self) -> String {
//         if let Some(tracks) = &self.album.tracks {
//             let duration_secs: u64 = tracks.iter().map(|t| t.duration as u64 / 1000).sum();
//             let duration = std::time::Duration::from_secs(duration_secs);
//             let duration_str = crate::utils::format_duration(&duration);
//             format!("{} tracks, {}", tracks.len(), duration_str)
//         } else {
//             "".to_string()
//         }
//     }

//     fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
//         self.tabs.on_command(s, cmd)
//     }
// }
