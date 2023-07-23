use std::sync::Arc;

use cursive::view::{Margins, ViewWrapper};
use cursive::views::{Dialog, NamedView, ScrollView, SelectView};
use cursive::Cursive;

use crate::commands::CommandResult;
use crate::ext_traits::SelectViewExt;
use crate::library::Library;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::queue::Queue;
#[cfg(feature = "share_clipboard")]
use crate::sharing::write_share;
use crate::spotify::PlayerEvent;
use crate::traits::{ListItem, ViewExt};
use crate::ui::layout::Layout;
use crate::ui::modal::Modal;
use crate::{command::Command, spotify::Spotify};
use cursive::traits::{Finder, Nameable};

pub struct ContextMenu {
    dialog: Modal<Dialog>,
}

pub struct AddToPlaylistMenu {
    dialog: Modal<Dialog>,
}

pub struct SelectArtistMenu {
    dialog: Modal<Dialog>,
}

pub struct SelectArtistActionMenu {
    dialog: Modal<Dialog>,
}

enum ContextMenuAction {
    ShowItem(Box<dyn ListItem>),
    SelectArtist(Vec<Artist>),
    SelectArtistAction(Artist),
    #[cfg(feature = "share_clipboard")]
    ShareUrl(String),
    AddToPlaylist(Box<Track>),
    ShowRecommendations(Box<Track>),
    ToggleSavedStatus(Box<dyn ListItem>),
    Play(Box<dyn ListItem>),
    PlayNext(Box<dyn ListItem>),
    TogglePlayback,
    Queue(Box<dyn ListItem>),
}

impl ContextMenu {
    pub fn add_track_dialog(
        library: Arc<Library>,
        spotify: Spotify,
        track: Track,
    ) -> NamedView<AddToPlaylistMenu> {
        let mut list_select: SelectView<Playlist> = SelectView::new();
        let current_user_id = library.user_id.as_ref().unwrap();

        for list in library.playlists().iter() {
            if current_user_id == &list.owner_id || list.collaborative {
                list_select.add_item(list.name.clone(), list.clone());
            }
        }

        list_select.set_on_submit(move |s, selected| {
            let track = track.clone();
            let mut playlist = selected.clone();
            let spotify = spotify.clone();
            let library = library.clone();

            if playlist.has_track(track.id.as_ref().unwrap_or(&String::new())) {
                let mut already_added_dialog = Self::track_already_added();

                already_added_dialog.add_button("Add anyway", move |c| {
                    let mut playlist = playlist.clone();

                    playlist.append_tracks(&[Playable::Track(track.clone())], &spotify, &library);
                    c.pop_layer();

                    // Close add_track_dialog too
                    c.pop_layer();
                });

                let modal = Modal::new(already_added_dialog);
                s.add_layer(modal);
            } else {
                playlist.append_tracks(&[Playable::Track(track)], &spotify, &library);
                s.pop_layer();
            }
        });

        let dialog = Dialog::new()
            .title("Add track to playlist")
            .dismiss_button("Close")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .content(ScrollView::new(list_select.with_name("addplaylist_select")));

        AddToPlaylistMenu {
            dialog: Modal::new_ext(dialog),
        }
        .with_name("addtrackmenu")
    }

    pub fn select_artist_dialog(
        library: Arc<Library>,
        queue: Arc<Queue>,
        artists: Vec<Artist>,
    ) -> NamedView<SelectArtistMenu> {
        let mut artist_select = SelectView::<Artist>::new();

        for artist in artists {
            artist_select.add_item(artist.name.clone(), artist);
        }

        artist_select.set_on_submit(move |s, selected_artist| {
            let dialog = Self::select_artist_action_dialog(
                library.clone(),
                queue.clone(),
                selected_artist.clone(),
            );
            s.pop_layer();
            s.add_layer(dialog);
        });

        let dialog = Dialog::new()
            .title("Select artist")
            .dismiss_button("Close")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .content(ScrollView::new(artist_select.with_name("artist_select")));

        SelectArtistMenu {
            dialog: Modal::new_ext(dialog),
        }
        .with_name("selectartist")
    }

    pub fn select_artist_action_dialog(
        library: Arc<Library>,
        queue: Arc<Queue>,
        artist: Artist,
    ) -> NamedView<SelectArtistActionMenu> {
        let moved_artist = artist.clone();
        let mut artist_action_select = SelectView::<bool>::new();
        artist_action_select.add_item("View Artist", true);
        artist_action_select.add_item(
            format!(
                "{}ollow Artist",
                if library.is_followed_artist(&artist) {
                    "Unf"
                } else {
                    "F"
                }
            ),
            false,
        );
        artist_action_select.set_on_submit(move |s, selected_action| {
            match selected_action {
                true => {
                    if let Some(view) = moved_artist.clone().open(queue.clone(), library.clone()) {
                        s.call_on_name("main", |v: &mut Layout| v.push_view(view));
                    }
                }
                false => {
                    if library.clone().is_followed_artist(&moved_artist) {
                        moved_artist.clone().unsave(&library);
                    } else {
                        moved_artist.clone().save(&library);
                    }
                }
            }
            s.pop_layer();
        });
        let dialog = Dialog::new()
            .title(format!(
                "Select action for artist: {}",
                artist.name.as_str()
            ))
            .dismiss_button("Close")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .content(ScrollView::new(
                artist_action_select.with_name("artist_action_select"),
            ));
        SelectArtistActionMenu {
            dialog: Modal::new_ext(dialog),
        }
        .with_name("selectartistaction")
    }

    fn track_already_added() -> Dialog {
        Dialog::text("This track is already in your playlist")
            .title("Track already exists")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .dismiss_button("Close")
    }

    pub fn new(item: &dyn ListItem, queue: Arc<Queue>, library: Arc<Library>) -> NamedView<Self> {
        let mut content: SelectView<ContextMenuAction> = SelectView::new();
        let album = item.album(&queue);

        if item.is_playable() {
            if item.is_playing(&queue)
                && queue.get_spotify().get_current_status()
                    == PlayerEvent::Paused(queue.get_spotify().get_current_progress())
            {
                // the item is the current track, but paused
                content.insert_item(0, "Resume", ContextMenuAction::TogglePlayback);
            } else if !item.is_playing(&queue) {
                // the item is not the current track
                content.insert_item(0, "Play", ContextMenuAction::Play(item.as_listitem()));
            } else {
                // the item is the current track and playing
                content.insert_item(0, "Pause", ContextMenuAction::TogglePlayback);
            }
            content.insert_item(
                1,
                "Play next",
                ContextMenuAction::PlayNext(item.as_listitem()),
            );
            content.insert_item(2, "Queue", ContextMenuAction::Queue(item.as_listitem()));
        }

        if let Some(artists) = item.artists() {
            let action = match artists.len() {
                0 => None,
                1 => Some(ContextMenuAction::SelectArtistAction(artists[0].clone())),
                _ => Some(ContextMenuAction::SelectArtist(artists.clone())),
            };

            if let Some(a) = action {
                content.add_item(
                    format!("Artist{}", if artists.len() > 1 { "s" } else { "" }),
                    a,
                )
            }
        }

        if let Some(ref a) = album {
            content.add_item(
                "Show album",
                ContextMenuAction::ShowItem(Box::new(a.clone())),
            );
        }

        #[cfg(feature = "share_clipboard")]
        {
            if let Some(url) = item.share_url() {
                content.add_item("Share", ContextMenuAction::ShareUrl(url));
            }
            if let Some(url) = album.as_ref().and_then(|a| a.share_url()) {
                content.add_item("Share album", ContextMenuAction::ShareUrl(url));
            }
        }

        if let Some(t) = item.track() {
            content.add_item(
                "Add to playlist",
                ContextMenuAction::AddToPlaylist(Box::new(t.clone())),
            );
            content.add_item(
                "Similar tracks",
                ContextMenuAction::ShowRecommendations(Box::new(t)),
            )
        }
        // If the item is saveable, its save state will be set
        if let Some(savestatus) = item.is_saved(&library) {
            content.add_item(
                match savestatus {
                    true => "Unsave",
                    false => "Save",
                },
                ContextMenuAction::ToggleSavedStatus(item.as_listitem()),
            );
        }

        if let Some(ref a) = album {
            if let Some(savestatus) = a.is_saved(&library) {
                content.add_item(
                    match savestatus {
                        true => "Unsave album",
                        false => "Save album",
                    },
                    ContextMenuAction::ToggleSavedStatus(a.as_listitem()),
                );
            }
        }

        // open detail view of artist/album
        {
            let library = library.clone();
            content.set_on_submit(move |s: &mut Cursive, action: &ContextMenuAction| {
                let queue = queue.clone();
                let library = library.clone();
                s.pop_layer();

                match action {
                    ContextMenuAction::ShowItem(item) => {
                        if let Some(view) = item.open(queue, library) {
                            s.call_on_name("main", move |v: &mut Layout| v.push_view(view));
                        }
                    }
                    #[cfg(feature = "share_clipboard")]
                    ContextMenuAction::ShareUrl(url) => {
                        write_share(url.to_string());
                    }
                    ContextMenuAction::AddToPlaylist(track) => {
                        let dialog =
                            Self::add_track_dialog(library, queue.get_spotify(), *track.clone());
                        s.add_layer(dialog);
                    }
                    ContextMenuAction::ShowRecommendations(item) => {
                        if let Some(view) = item.to_owned().open_recommendations(queue, library) {
                            s.call_on_name("main", move |v: &mut Layout| v.push_view(view));
                        }
                    }
                    ContextMenuAction::SelectArtist(artists) => {
                        let dialog = Self::select_artist_dialog(library, queue, artists.clone());
                        s.add_layer(dialog);
                    }
                    ContextMenuAction::SelectArtistAction(artist) => {
                        let dialog =
                            Self::select_artist_action_dialog(library, queue, artist.clone());
                        s.add_layer(dialog);
                    }
                    ContextMenuAction::ToggleSavedStatus(item) => {
                        item.as_listitem().toggle_saved(&library)
                    }
                    ContextMenuAction::Play(item) => item.as_listitem().play(&queue),
                    ContextMenuAction::PlayNext(item) => item.as_listitem().play_next(&queue),
                    ContextMenuAction::TogglePlayback => queue.toggleplayback(),
                    ContextMenuAction::Queue(item) => item.as_listitem().queue(&queue),
                }
            });
        }

        let dialog = Dialog::new()
            .title(item.display_left(&library))
            .dismiss_button("Close")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .content(content.with_name("contextmenu_select"));
        Self {
            dialog: Modal::new_ext(dialog),
        }
        .with_name("contextmenu")
    }
}

impl ViewExt for AddToPlaylistMenu {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        log::info!("playlist command: {:?}", cmd);
        handle_move_command::<Playlist>(&mut self.dialog, s, cmd, "addplaylist_select")
    }
}

impl ViewExt for ContextMenu {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        handle_move_command::<ContextMenuAction>(&mut self.dialog, s, cmd, "contextmenu_select")
    }
}

impl ViewExt for SelectArtistMenu {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        log::info!("artist move command: {:?}", cmd);
        handle_move_command::<Artist>(&mut self.dialog, s, cmd, "artist_select")
    }
}

impl ViewExt for SelectArtistActionMenu {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        handle_move_command::<bool>(&mut self.dialog, s, cmd, "artist_action_select")
    }
}

fn handle_move_command<T: 'static>(
    sel: &mut Modal<Dialog>,
    s: &mut Cursive,
    cmd: &Command,
    name: &str,
) -> Result<CommandResult, String> {
    match cmd {
        Command::Back => {
            s.pop_layer();
            Ok(CommandResult::Consumed(None))
        }
        Command::Move(_, _) => sel
            .call_on_name(name, |select: &mut SelectView<T>| {
                select.handle_command(cmd)
            })
            .unwrap_or(Ok(CommandResult::Consumed(None))),
        _ => Ok(CommandResult::Consumed(None)),
    }
}

impl ViewWrapper for AddToPlaylistMenu {
    wrap_impl!(self.dialog: Modal<Dialog>);
}

impl ViewWrapper for ContextMenu {
    wrap_impl!(self.dialog: Modal<Dialog>);
}

impl ViewWrapper for SelectArtistMenu {
    wrap_impl!(self.dialog: Modal<Dialog>);
}

impl ViewWrapper for SelectArtistActionMenu {
    wrap_impl!(self.dialog: Modal<Dialog>);
}
