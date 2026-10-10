use std::sync::{Arc, RwLock};
use std::thread;

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
        let items = Arc::new(RwLock::new(Vec::new()));
        let list = ListView::new(items.clone(), queue.clone(), library.clone());
        let pagination = list.get_pagination().clone();

        // load in the background, a rate limit would otherwise block startup
        thread::spawn(move || {
            let mut categories = queue.get_spotify().api.categories();
            // fill the list before arming pagination, so pages stay in order
            items
                .write()
                .unwrap()
                .append(&mut categories.items.write().unwrap());
            categories.items = items;
            categories.apply_pagination(&pagination);
            library.trigger_redraw();
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
