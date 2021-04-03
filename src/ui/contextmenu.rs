use std::sync::Arc;

use cursive::view::{Margins, ViewWrapper};
use cursive::views::{Dialog, NamedView, ScrollView, SelectView};
use cursive::Cursive;

use crate::library::Library;
use crate::playable::Playable;
use crate::queue::Queue;
#[cfg(feature = "share_clipboard")]
use crate::sharing::write_share;
use crate::track::Track;
use crate::traits::{ListItem, ViewExt};
use crate::ui::layout::Layout;
use crate::ui::modal::Modal;
use crate::{artist::Artist, commands::CommandResult};
use crate::{
    command::{Command, MoveAmount, MoveMode},
    playlist::Playlist,
    spotify::Spotify,
};
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

enum ContextMenuAction {
    ShowItem(Box<dyn ListItem>),
    SelectArtist(Vec<Artist>),
    ShareUrl(String),
    AddToPlaylist(Box<Track>),
    ShowRecommendations(Box<dyn ListItem>),
    ToggleTrackSavedStatus(Box<Track>),
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
                    let spotify = spotify.clone();
                    let library = library.clone();

                    playlist.append_tracks(&[track.clone()], spotify, library);
                    // playlist.map(|p| p.append_tracks(&[track.clone()], spotify, library));
                    c.pop_layer();

                    // Close add_track_dialog too
                    c.pop_layer();
                });

                let modal = Modal::new(already_added_dialog);
                s.add_layer(modal);
            } else {
                playlist.append_tracks(&[track], spotify, library);
                s.pop_layer();
            }
        });

        let dialog = Dialog::new()
            .title("Add track to playlist")
            .dismiss_button("Cancel")
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
        let mut list_select = SelectView::<Artist>::new();

        for artist in artists {
            list_select.add_item(artist.name.clone(), artist);
        }

        list_select.set_on_submit(move |s, selected| {
            if let Some(view) = selected.open(queue.clone(), library.clone()) {
                s.call_on_name("main", move |v: &mut Layout| v.push_view(view));
                s.pop_layer();
            }
        });

        let dialog = Dialog::new()
            .title("Select artist")
            .dismiss_button("Cancel")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .content(ScrollView::new(list_select.with_name("artist_select")));

        SelectArtistMenu {
            dialog: Modal::new_ext(dialog),
        }
        .with_name("selectartist")
    }

    fn track_already_added() -> Dialog {
        Dialog::text("This track is already in your playlist")
            .title("Track already exists")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .dismiss_button("Cancel")
    }

    pub fn new(item: &dyn ListItem, queue: Arc<Queue>, library: Arc<Library>) -> NamedView<Self> {
        let mut content: SelectView<ContextMenuAction> = SelectView::new();
        if let Some(a) = item.artists() {
            let action = match a.len() {
                0 => None,
                1 => Some(ContextMenuAction::ShowItem(Box::new(a[0].clone()))),
                _ => Some(ContextMenuAction::SelectArtist(a)),
            };

            if let Some(a) = action {
                content.add_item("Show artist", a)
            }
        }
        if let Some(a) = item.album(queue.clone()) {
            content.add_item("Show album", ContextMenuAction::ShowItem(Box::new(a)));
        }
        if let Some(url) = item.share_url() {
            #[cfg(feature = "share_clipboard")]
            content.add_item("Share", ContextMenuAction::ShareUrl(url));
        }
        if let Some(t) = item.track() {
            content.add_item(
                "Add to playlist",
                ContextMenuAction::AddToPlaylist(Box::new(t.clone())),
            );
            content.add_item(
                "Similar tracks",
                ContextMenuAction::ShowRecommendations(Box::new(t.clone())),
            );
            content.add_item(
                match library.is_saved_track(&Playable::Track(t.clone())) {
                    true => "Unsave track",
                    false => "Save track",
                },
                ContextMenuAction::ToggleTrackSavedStatus(Box::new(t)),
            )
        }

        // open detail view of artist/album
        content.set_on_submit(move |s: &mut Cursive, action: &ContextMenuAction| {
            s.pop_layer();
            let queue = queue.clone();
            let library = library.clone();

            match action {
                ContextMenuAction::ShowItem(item) => {
                    if let Some(view) = item.open(queue, library) {
                        s.call_on_name("main", move |v: &mut Layout| v.push_view(view));
                    }
                }
                ContextMenuAction::ShareUrl(url) => {
                    #[cfg(feature = "share_clipboard")]
                    write_share(url.to_string());
                }
                ContextMenuAction::AddToPlaylist(track) => {
                    let dialog =
                        Self::add_track_dialog(library, queue.get_spotify(), *track.clone());
                    s.add_layer(dialog);
                }
                ContextMenuAction::ShowRecommendations(item) => {
                    if let Some(view) = item.open_recommendations(queue, library) {
                        s.call_on_name("main", move |v: &mut Layout| v.push_view(view));
                    }
                }
                ContextMenuAction::ToggleTrackSavedStatus(track) => {
                    let mut track: Track = *track.clone();
                    track.toggle_saved(library);
                }
                ContextMenuAction::SelectArtist(artists) => {
                    let dialog = Self::select_artist_dialog(library, queue, artists.clone());
                    s.add_layer(dialog);
                }
            }
        });

        let dialog = Dialog::new()
            .title(item.display_left())
            .dismiss_button("Cancel")
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
        handle_move_command::<Playlist>(&mut self.dialog, s, cmd, "addplaylist_select")
    }
}

impl ViewExt for ContextMenu {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        handle_move_command::<ContextMenuAction>(&mut self.dialog, s, cmd, "contextmenu_select")
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
        Command::Move(mode, amount) => sel
            .call_on_name(name, |select: &mut SelectView<T>| {
                let items = select.len();
                match mode {
                    MoveMode::Up => {
                        match amount {
                            MoveAmount::Extreme => select.set_selection(0),
                            MoveAmount::Integer(amount) => select.select_up(*amount as usize),
                        };
                        Ok(CommandResult::Consumed(None))
                    }
                    MoveMode::Down => {
                        match amount {
                            MoveAmount::Extreme => select.set_selection(items),
                            MoveAmount::Integer(amount) => select.select_down(*amount as usize),
                        };
                        Ok(CommandResult::Consumed(None))
                    }
                    _ => Ok(CommandResult::Consumed(None)),
                }
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
