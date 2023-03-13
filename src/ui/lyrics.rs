use std::sync::Arc;

use cursive::{
    theme::Effect,
    utils::markup::StyledString,
    view::ViewWrapper,
    views::{DummyView, LinearLayout, ScrollView, TextView},
};

use crate::{commands::CommandResult, lyrics::LyricsManager, traits::ViewExt, command::Command};

pub struct LyricsView {
    manager: Arc<LyricsManager>,
    view: LinearLayout,
}

impl LyricsView {
    pub fn new(manager: Arc<LyricsManager>) -> LyricsView {
        let mut text = StyledString::styled("Keybindings\n\n", Effect::Bold);

        let note = format!(
            "Custom bindings can be set in the {} file within the [keybindings] section.\n\n",
            "test"
        );

        // TODO: fixme
        let content = String::from("");

        text.append(StyledString::styled(note, Effect::Italic));
        text.append(content);

        text.append("\n\n");
        text.append(StyledString::styled(
            manager.get_lyrics_for_current(),
            Effect::Simple,
        ));

        let lyrics_view = ScrollView::new(TextView::new(text).center());

        let view = LinearLayout::vertical()
            .child(TextView::new("Title").center())
            .child(TextView::new("Authors").center())
            .child(TextView::new("Album").center())
            .child(DummyView)
            .child(lyrics_view);

        LyricsView { manager, view }
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

    fn on_leave(&self) {}

    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Save => self.save_lyrics(),
            _ => Ok(CommandResult::Ignored),
        }
    }
}
