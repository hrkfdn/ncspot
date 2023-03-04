use std::sync::Arc;

use cursive::view::{View, ViewWrapper};
use cursive::views::NamedView;
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::track::Track;
use crate::queue::Queue;

pub trait ListItem: Sync + Send + 'static {
    fn is_playing(&self, queue: &Queue) -> bool;
    fn display_left(&self, library: &Library) -> String;
    fn display_center(&self, _library: &Library) -> String {
        "".to_string()
    }
    fn display_right(&self, library: &Library) -> String;
    fn play(&mut self, queue: &Queue);
    fn play_next(&mut self, queue: &Queue);
    fn queue(&mut self, queue: &Queue);
    fn toggle_saved(&mut self, library: &Library);
    fn save(&mut self, library: &Library);
    fn unsave(&mut self, library: &Library);
    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>>;
    fn open_recommendations(
        &mut self,
        _queue: Arc<Queue>,
        _library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        None
    }
    fn share_url(&self) -> Option<String>;

    fn album(&self, _queue: &Queue) -> Option<Album> {
        None
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        None
    }

    fn track(&self) -> Option<Track> {
        None
    }

    #[allow(unused_variables)]
    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        None
    }

    #[inline]
    fn is_playable(&self) -> bool {
        false
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
