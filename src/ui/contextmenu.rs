use std::sync::Arc;

use cursive::view::{Margins, ViewWrapper};
use cursive::views::{Dialog, NamedView, ScrollView, SelectView};
use cursive::Cursive;

use crate::commands::CommandResult;
use crate::library::Library;
use crate::queue::Queue;
use crate::track::Track;
use crate::traits::{ListItem, ViewExt};
use crate::playable::Playable;
use crate::ui::layout::Layout;
use crate::ui::modal::Modal;
use crate::{
    command::{Command, MoveAmount, MoveMode},
    playlist::Playlist,
    spotify::Spotify,
};
#[cfg(feature = "share_clipboard")]
use clipboard::{ClipboardContext, ClipboardProvider};
use cursive::traits::{Finder, Nameable};

pub struct ContextMenu {
    dialog: Modal<Dialog>,
}

enum ContextMenuAction {
    ShowItem(Box<dyn ListItem>),
    ShareUrl(String),
    AddToPlaylist(Box<Track>),
    ShowRecommentations(Box<dyn ListItem>),
    ToggleTrackSavedStatus(Box<Track>),
}

impl ContextMenu {
    pub fn add_track_dialog(
        library: Arc<Library>,
        spotify: Arc<Spotify>,
        track: Track,
    ) -> Modal<Dialog> {
        let mut list_select: SelectView<Playlist> = SelectView::new().autojump();
        let current_user_id = library.user_id.as_ref().unwrap();

        for list in library.items().iter() {
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
            .content(ScrollView::new(list_select));
        Modal::new(dialog)
    }

    fn track_already_added() -> Dialog {
        Dialog::text("This track is already in your playlist")
            .title("Track already exists")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .dismiss_button("Cancel")
    }

    pub fn new(item: &dyn ListItem, queue: Arc<Queue>, library: Arc<Library>) -> NamedView<Self> {
        let mut content: SelectView<ContextMenuAction> = SelectView::new().autojump();
        if let Some(a) = item.artist() {
            content.add_item("Show artist", ContextMenuAction::ShowItem(Box::new(a)));
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
                ContextMenuAction::ShowRecommentations(Box::new(t.clone())),
            );
            content.add_item(
                match library.is_saved_track(&Playable::Track(t.clone())) {
                    true => "Unsave track",
                    false => "Save track",
                },
                ContextMenuAction::ToggleTrackSavedStatus(Box::new(t.clone())),
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
                    ClipboardProvider::new()
                        .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url.to_string()))
                        .ok();
                }
                ContextMenuAction::AddToPlaylist(track) => {
                    let dialog =
                        Self::add_track_dialog(library, queue.get_spotify(), *track.clone());
                    s.add_layer(dialog);
                }
                ContextMenuAction::ShowRecommentations(item) => {
                    if let Some(view) = item.open_recommentations(queue, library) {
                        s.call_on_name("main", move |v: &mut Layout| v.push_view(view));
                    }
                }
                ContextMenuAction::ToggleTrackSavedStatus(track) => {
                    let mut track: Track = *track.clone();
                    track.toggle_saved(library);
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

impl ViewExt for ContextMenu {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Back => {
                s.pop_layer();
                Ok(CommandResult::Consumed(None))
            }
            Command::Move(mode, amount) => self
                .dialog
                .call_on_name(
                    "contextmenu_select",
                    |select: &mut SelectView<ContextMenuAction>| {
                        let items = select.len();
                        match mode {
                            MoveMode::Up => {
                                match amount {
                                    MoveAmount::Extreme => select.set_selection(0),
                                    MoveAmount::Integer(amount) => {
                                        select.select_up(*amount as usize)
                                    }
                                };
                                Ok(CommandResult::Consumed(None))
                            }
                            MoveMode::Down => {
                                match amount {
                                    MoveAmount::Extreme => select.set_selection(items),
                                    MoveAmount::Integer(amount) => {
                                        select.select_down(*amount as usize)
                                    }
                                };
                                Ok(CommandResult::Consumed(None))
                            }
                            _ => Ok(CommandResult::Consumed(None)),
                        }
                    },
                )
                .unwrap_or(Ok(CommandResult::Consumed(None))),
            _ => Ok(CommandResult::Consumed(None)),
        }
    }
}

impl ViewWrapper for ContextMenu {
    wrap_impl!(self.dialog: Modal<Dialog>);
}
