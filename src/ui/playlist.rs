// use std::sync::{Arc, RwLock};

// use cursive::view::ViewWrapper;
// use cursive::Cursive;
// use cursive::views::ScrollView;

// use crate::command::Command;
// use crate::commands::CommandResult;
// use crate::library::Library;
// use crate::model::playable::Playable;
// use crate::model::playlist::Playlist;
// use crate::queue::Queue;
// use crate::spotify::Spotify;
// use crate::traits::ViewExt;

// use super::list::List;

// pub struct PlaylistView {
//     playlist: Playlist,
//     list: ScrollView<List<Playable>>,
//     spotify: Spotify,
//     library: Arc<Library>,
//     queue: Arc<Queue>,
// }

// impl PlaylistView {
//     pub fn new(queue: Arc<Queue>, library: Arc<Library>, playlist: &Playlist) -> Self {
//         let mut playlist = playlist.clone();
//         playlist.load_tracks(queue.get_spotify());

//         if let Some(order) = library.cfg.state().playlist_orders.get(&playlist.id) {
//             playlist.sort(&order.key, &order.direction);
//         }

//         let tracks = if let Some(t) = playlist.tracks.as_ref() {
//             t.clone()
//         } else {
//             Vec::new()
//         };

//         let spotify = queue.get_spotify();
//         let list = List::new(
//             Arc::new(RwLock::new(tracks)),
//         );

//         Self {
//             playlist,
//             list: ScrollView::new(list),
//             spotify,
//             library,
//             queue,
//         }
//     }
// }

// impl ViewWrapper for PlaylistView {
//     wrap_impl!(self.list: ScrollView<List<Playable>>);
// }

// impl ViewExt for PlaylistView {
//     fn title(&self) -> String {
//         self.playlist.name.clone()
//     }

//     fn title_sub(&self) -> String {
//         if let Some(tracks) = self.playlist.tracks.as_ref() {
//             let duration_secs = tracks.iter().map(|p| p.duration() as u64 / 1000).sum();
//             let duration = std::time::Duration::from_secs(duration_secs);
//             format!(
//                 "{} tracks, {}",
//                 tracks.len(),
//                 crate::utils::format_duration(&duration)
//             )
//         } else {
//             "".to_string()
//         }
//     }

//     fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
//         if let Command::Delete = cmd {
//             let pos = self.list.get_inner().selected_index();
//             if self
//                 .playlist
//                 .delete_track(pos, self.spotify.clone(), self.library.clone())
//             {
//                 self.list.get_inner_mut().remove(pos);
//             }
//             return Ok(CommandResult::Consumed(None));
//         }

//         if let Command::Sort(key, direction) = cmd {
//             self.library.cfg.with_state_mut(|mut state| {
//                 let order = crate::config::SortingOrder {
//                     key: key.clone(),
//                     direction: direction.clone(),
//                 };
//                 state
//                     .playlist_orders
//                     .insert(self.playlist.id.clone(), order);
//             });

//             self.playlist.sort(key, direction);
//             let tracks = self.playlist.tracks.as_ref().unwrap_or(&Vec::new()).clone();
//             self.list = ScrollView::new(List::new(
//                 Arc::new(RwLock::new(tracks)),
//             ));
//             return Ok(CommandResult::Consumed(None));
//         }

//         self.list.on_command(s, cmd)
//     }
// }
