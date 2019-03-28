use std::sync::Arc;

use cursive::view::{View, ViewWrapper};
use cursive::views::IdView;
use cursive::Cursive;

use commands::CommandResult;
use queue::Queue;

pub trait ListItem {
    fn is_playing(&self, queue: Arc<Queue>) -> bool;
    fn display_left(&self) -> String;
    fn display_right(&self) -> String;
    fn play(&self, queue: Arc<Queue>);
    fn queue(&self, queue: Arc<Queue>);
}

pub trait ViewExt: View {
    fn on_command(
        &mut self,
        _s: &mut Cursive,
        _cmd: &str,
        _args: &[String],
    ) -> Result<CommandResult, String> {
        Ok(CommandResult::Ignored)
    }
}

impl<V: ViewExt> ViewExt for IdView<V> {
    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        self.with_view_mut(move |v| v.on_command(s, cmd, args))
            .unwrap()
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
