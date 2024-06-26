use std::sync::Arc;

use cursive::{
    theme::Effect,
    view::ViewWrapper,
    views::{DummyView, LinearLayout, ResizedView, ScrollView, TextContent, TextView},
};

use crate::{command::Command, commands::CommandResult, lyrics::LyricsManager, traits::ViewExt};

pub struct LyricsView {
    manager: Arc<LyricsManager>,
    view: LinearLayout,
    track_lyrics: TextContent,
    track_title: TextContent,
    track_authors: TextContent,
    track_album: TextContent,
}

impl LyricsView {
    pub fn new(manager: Arc<LyricsManager>) -> LyricsView {
        // INITIALIZE THESE WITH "TRASHY" VALUE THAT IS GOING TO BE REPLACED AFTER
        let track_title = TextContent::new("No track being played");
        let track_authors = TextContent::new("No track being played");
        let track_album = TextContent::new("No track being played");
        let track_lyrics = TextContent::new("No track being played");

        let lyrics_view =
            ScrollView::new(TextView::new_with_content(track_lyrics.clone()).center());

        let view = LinearLayout::vertical()
            .child(ResizedView::with_full_width(
                ResizedView::with_fixed_height(5, DummyView),
            ))
            .child(
                TextView::new_with_content(track_title.clone())
                    .center()
                    .style(Effect::Bold),
            )
            .child(TextView::new_with_content(track_authors.clone()).center())
            .child(
                TextView::new_with_content(track_album.clone())
                    .center()
                    .style(Effect::Italic),
            )
            .child(DummyView)
            .child(lyrics_view);

        let lyrics_view = LyricsView {
            manager,
            view,
            track_lyrics,
            track_album,
            track_authors,
            track_title,
        };

        lyrics_view.update_lyrics();

        lyrics_view
    }

    fn update_lyrics(&self) {
        // TODO: this should be done in a separate thread and the UI should be updated when the lyrics are fetched (or an error occurs)

        let current_track = self.manager.get_current_track();

        if let Some(track) = current_track {
            let track_title_str = track.clone().title;

            let track_authors_str = track.artists.join(", ");

            let track_album_str = match track.clone().album {
                None => String::default(),
                Some(album_name) => album_name,
            };

            let track_lyrics_str = self.manager.get_lyrics(track);

            self.track_title.set_content(track_title_str);
            self.track_authors.set_content(track_authors_str);
            self.track_album.set_content(track_album_str);
            self.track_lyrics.set_content(track_lyrics_str);
        }
    }

    /// Saves the lyrics of the current song
    pub fn save_lyrics(&self) -> Result<CommandResult, String> {
        let result = self
            .manager
            .save_lyrics(self.manager.get_lyrics_for_current());

        Ok(CommandResult::Consumed(result))
    }
}

impl ViewWrapper for LyricsView {
    wrap_impl!(self.view: LinearLayout);
}

impl ViewExt for LyricsView {
    fn title(&self) -> String {
        "Lyrics".to_string()
    }

    fn title_sub(&self) -> String {
        "".to_string()
    }

    fn on_enter(&mut self) {
        self.update_lyrics();
    }

    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Save => self.save_lyrics(),
            Command::Next | Command::Previous => {
                // still does not work
                Ok(CommandResult::Ignored) // return ignored so it is processed by the default command handler
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}
