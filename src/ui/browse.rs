use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::views::ScrollView;
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::model::category::Category;
use crate::queue::Queue;
use crate::traits::ViewExt;

use super::list::List;

pub struct BrowseView {
    list: ScrollView<List<Category>>,
}

impl BrowseView {
    pub fn new(queue: Arc<Queue>) -> Self {
        let categories = queue.get_spotify().api.categories();
        let list = ScrollView::new(List::new(categories.items));
        // FIX: categories.apply_pagination(list.get_pagination()) used to be
        // here!

        Self { list }
    }
}

impl ViewWrapper for BrowseView {
    wrap_impl!(self.list: ScrollView<List<Category>>);
}

impl ViewExt for BrowseView {
    fn title(&self) -> String {
        "Browse".to_string()
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.list.on_command(s, cmd)
    }
}
