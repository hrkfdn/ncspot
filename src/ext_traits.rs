use cursive::views::ViewRef;
use cursive::{View, XY};

use crate::command::{Command, MoveAmount, MoveMode};
use crate::commands::CommandResult;
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

pub trait SelectViewExt {
    /// Translates ncspot commands (i.e. navigating in lists) to Cursive
    /// `SelectView` actions.
    fn handle_command(&mut self, cmd: &Command) -> Result<CommandResult, String>;
}

impl<T: 'static> SelectViewExt for cursive::views::SelectView<T> {
    fn handle_command(&mut self, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Move(mode, amount) => {
                let items = self.len();
                match mode {
                    MoveMode::Up => {
                        match amount {
                            MoveAmount::Extreme => self.set_selection(0),
                            MoveAmount::Float(scale) => {
                                let amount = (*self).required_size(XY::default()).y as f32 * scale;
                                self.select_up(amount as usize)
                            }
                            MoveAmount::Integer(amount) => self.select_up(*amount as usize),
                        };
                        Ok(CommandResult::Consumed(None))
                    }
                    MoveMode::Down => {
                        match amount {
                            MoveAmount::Extreme => self.set_selection(items),
                            MoveAmount::Float(scale) => {
                                let amount = (*self).required_size(XY::default()).y as f32 * scale;
                                self.select_down(amount as usize)
                            }
                            MoveAmount::Integer(amount) => self.select_down(*amount as usize),
                        };
                        Ok(CommandResult::Consumed(None))
                    }
                    _ => Ok(CommandResult::Consumed(None)),
                }
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}
