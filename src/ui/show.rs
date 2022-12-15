// use std::sync::Arc;

// use cursive::view::ViewWrapper;
// use cursive::views::ScrollView;
// use cursive::Cursive;

// use crate::command::Command;
// use crate::commands::CommandResult;
// use crate::library::Library;
// use crate::model::episode::Episode;
// use crate::model::show::Show;
// use crate::queue::Queue;
// use crate::traits::ViewExt;

// use super::list::List;

// pub struct ShowView {
//     list: ScrollView<List<Episode>>,
//     show: Show,
// }

// impl ShowView {
//     pub fn new(queue: Arc<Queue>, library: Arc<Library>, show: &Show) -> Self {
//         let spotify = queue.get_spotify();
//         let show = show.clone();

//         let list = {
//             let results = spotify.api.show_episodes(&show.id);
//             let view = List::new(results.items.clone());
//             // FIX: results.apply_pagination(view.get_pagination()) used to be
//             // here!

//             view
//         };

//         Self {
//             list: ScrollView::new(list),
//             show,
//         }
//     }
// }

// impl ViewWrapper for ShowView {
//     wrap_impl!(self.list: ScrollView<List<Episode>>);
// }

// impl ViewExt for ShowView {
//     fn title(&self) -> String {
//         self.show.name.clone()
//     }

//     fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
//         self.list.on_command(s, cmd)
//     }
// }
