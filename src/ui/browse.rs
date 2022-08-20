use std::sync::{Arc, RwLock};

use cursive::view::ViewWrapper;
use cursive::Cursive;

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
        let items = Arc::new(RwLock::new(Vec::new()));
        let list = ListView::new(items.clone(), queue.clone(), library);

        let pagination = list.get_pagination().clone();
        std::thread::spawn(move || {
            let categories = queue.get_spotify().api.categories();
            items
                .write()
                .expect("could not writelock category items")
                .extend(
                    categories
                        .items
                        .read()
                        .expect("could not readlock fetched categories")
                        .clone(),
                );
            categories.apply_pagination(&pagination);
        });

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
