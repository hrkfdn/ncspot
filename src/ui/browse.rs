use std::sync::Arc;

use cursive::Cursive;
use cursive::view::ViewWrapper;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::category::Category;
use crate::queue::Queue;
use crate::traits::ViewExt;

use crate::ui::listview::ListView;

pub struct BrowseView {
    list: ListView<Category>,
}

impl BrowseView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let categories = queue.get_spotify().api.categories();
        let list = ListView::new(categories.items.clone(), queue, library);
        categories.apply_pagination(list.get_pagination());

        Self { list }
    }
}

impl ViewWrapper for BrowseView {
    wrap_impl!(self.list: ListView<Category>);
}

impl ViewExt for BrowseView {
    fn title(&self) -> String {
        "Browse".to_string()
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.list.on_command(s, cmd)
    }
}
