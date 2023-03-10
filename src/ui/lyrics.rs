use std::sync::Arc;

use cursive::{
    theme::Effect,
    utils::markup::StyledString,
    view::ViewWrapper,
    views::{ScrollView, TextView},
};

use crate::{commands::CommandResult, queue::Queue, traits::ViewExt, command::Command};

pub struct LyricsView {
    queue: Arc<Queue>,
    view: ScrollView<TextView>,
}

impl LyricsView {
    pub fn new(queue: Arc<Queue>) -> LyricsView {
        let mut text = StyledString::styled("Keybindings\n\n", Effect::Bold);

        let note = format!(
            "Custom bindings can be set in the {} file within the [keybindings] section.\n\n",
            "test"
        );
        text.append(StyledString::styled(note, Effect::Italic));

        LyricsView {
            queue,
            view: ScrollView::new(TextView::new(text)),
        }
    }

    pub fn save_lyrics(&mut self, lyrics: String) -> Result<CommandResult, String> {
        // println!("Saving Lyrics: {}", lyrics);

        self.view.get_inner_mut().set_content(lyrics);

        Ok(CommandResult::Consumed(None))
    }
}

impl ViewWrapper for LyricsView {
    wrap_impl!(self.view: ScrollView<TextView>);
}

impl ViewExt for LyricsView {
    fn title(&self) -> String {
        "Lyrics".to_string()
    }

    fn title_sub(&self) -> String {
        let current_track = self.queue.get_current().unwrap();

        format!("{}", current_track)
    }

    fn on_leave(&self) {}

    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Save => self.save_lyrics(format!("{}", cmd)),
            Command::Quit => Ok(CommandResult::Ignored),
            Command::Focus(_) => Ok(CommandResult::Ignored),
            _ => Ok(CommandResult::Ignored)
        }
    }
}
