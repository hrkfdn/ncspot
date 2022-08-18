use cursive::views::ViewRef;

use crate::ui::layout::Layout;

pub trait CursiveExt {
    fn on_layout<F, R>(&mut self, cb: F) -> R
    where
        F: FnOnce(&mut cursive::Cursive, ViewRef<Layout>) -> R;
}

impl CursiveExt for cursive::Cursive {
    fn on_layout<F, R>(&mut self, cb: F) -> R
    where
        F: FnOnce(&mut cursive::Cursive, ViewRef<Layout>) -> R,
    {
        let layout = self
            .find_name::<Layout>("main")
            .expect("Could not find Layout");
        cb(self, layout)
    }
}
