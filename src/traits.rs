use std::sync::Arc;

use cursive::view::{View, ViewWrapper};
use cursive::views::NamedView;
use cursive::Cursive;

use crate::album::Album;
use crate::artist::Artist;
use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::queue::Queue;
use crate::track::Track;

pub trait ListItem: Sync + Send + 'static {
    fn is_playing(&self, queue: Arc<Queue>) -> bool;
    fn display_left(&self) -> String;
    fn display_center(&self, _library: Arc<Library>) -> String {
        "".to_string()
    }
    fn display_right(&self, library: Arc<Library>) -> String;
    fn play(&mut self, queue: Arc<Queue>);
    fn queue(&mut self, queue: Arc<Queue>);
    fn play_next(&mut self, queue: Arc<Queue>);
    fn toggle_saved(&mut self, library: Arc<Library>);
    fn save(&mut self, library: Arc<Library>);
    fn unsave(&mut self, library: Arc<Library>);
    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>>;
    fn open_recommendations(
        &self,
        _queue: Arc<Queue>,
        _library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        None
    }
    fn share_url(&self) -> Option<String>;

    fn album(&self, _queue: Arc<Queue>) -> Option<Album> {
        None
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        None
    }

    fn track(&self) -> Option<Track> {
        None
    }

    fn as_listitem(&self) -> Box<dyn ListItem>;
}

pub trait ViewExt: View {
    fn title(&self) -> String {
        "".into()
    }

    fn title_sub(&self) -> String {
        "".into()
    }

    fn on_leave(&self) {}

    fn on_command(&mut self, _s: &mut Cursive, _cmd: &Command) -> Result<CommandResult, String> {
        Ok(CommandResult::Ignored)
    }
}

impl<V: ViewExt> ViewExt for NamedView<V> {
    fn title(&self) -> String {
        self.with_view(|v| v.title()).unwrap_or_default()
    }

    fn title_sub(&self) -> String {
        self.with_view(|v| v.title_sub()).unwrap_or_default()
    }

    fn on_leave(&self) {
        self.with_view(|v| v.on_leave());
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.with_view_mut(move |v| v.on_command(s, cmd)).unwrap()
    }
}

pub trait IntoBoxedViewExt {
    fn into_boxed_view_ext(self) -> Box<dyn ViewExt>;
}

impl<V: ViewExt> IntoBoxedViewExt for V {
    fn into_boxed_view_ext(self) -> Box<dyn ViewExt> {
        Box::new(self)
    }
}
