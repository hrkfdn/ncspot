use std::sync::Arc;

use cursive::view::{View, ViewWrapper};
use cursive::views::IdView;
use cursive::Cursive;

use album::Album;
use artist::Artist;
use command::Command;
use commands::CommandResult;
use library::Library;
use queue::Queue;

pub trait ListItem: Sync + Send + 'static {
    fn is_playing(&self, queue: Arc<Queue>) -> bool;
    fn display_left(&self) -> String;
    fn display_right(&self, library: Arc<Library>) -> String;
    fn play(&mut self, queue: Arc<Queue>);
    fn queue(&mut self, queue: Arc<Queue>);
    fn toggle_saved(&mut self, library: Arc<Library>);
    fn save(&mut self, library: Arc<Library>);
    fn unsave(&mut self, library: Arc<Library>);
    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>>;
    fn share_url(&self) -> Option<String>;

    fn album(&self, _queue: Arc<Queue>) -> Option<Album> {
        None
    }

    fn artist(&self) -> Option<Artist> {
        None
    }

    fn as_listitem(&self) -> Box<dyn ListItem>;
}

pub trait ViewExt: View {
    fn title(&self) -> String {
        "".into()
    }

    fn on_command(&mut self, _s: &mut Cursive, _cmd: &Command) -> Result<CommandResult, String> {
        Ok(CommandResult::Ignored)
    }
}

impl<V: ViewExt> ViewExt for IdView<V> {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.with_view_mut(move |v| v.on_command(s, cmd)).unwrap()
    }
}

pub trait IntoBoxedViewExt {
    fn as_boxed_view_ext(self) -> Box<dyn ViewExt>;
}

impl<V: ViewExt> IntoBoxedViewExt for V {
    fn as_boxed_view_ext(self) -> Box<dyn ViewExt> {
        Box::new(self)
    }
}
