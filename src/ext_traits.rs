use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use cursive::views::ViewRef;
use cursive::{View, XY};
use log::warn;

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
        F: FnOnce(&mut Self, ViewRef<Layout>) -> R,
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

impl<T: Send + Sync + 'static> SelectViewExt for cursive::views::SelectView<T> {
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

/// Lock access that survives poisoning. A thread that panics while holding a std lock marks
/// it poisoned, and every later `.read().unwrap()` panics too, so one failed background job
/// would take the UI down. These recover the guard instead and clear the flag: the data may
/// be mid-edit, which the writers keep rare by building changes before they take the lock.
pub trait RwLockExt<T> {
    fn read_unpoisoned(&self) -> RwLockReadGuard<'_, T>;
    fn write_unpoisoned(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    #[allow(clippy::disallowed_methods)]
    fn read_unpoisoned(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| {
            warn!("recovered a lock poisoned by an earlier panic");
            self.clear_poison();
            poisoned.into_inner()
        })
    }

    #[allow(clippy::disallowed_methods)]
    fn write_unpoisoned(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| {
            warn!("recovered a lock poisoned by an earlier panic");
            self.clear_poison();
            poisoned.into_inner()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use super::RwLockExt;

    fn poisoned() -> Arc<RwLock<Vec<u32>>> {
        let lock = Arc::new(RwLock::new(vec![1]));
        let held = Arc::clone(&lock);
        let joined = std::thread::spawn(move || {
            let mut guard = held.write_unpoisoned();
            guard.push(2);
            panic!("writer died holding the lock");
        })
        .join();
        assert!(joined.is_err());
        assert!(lock.is_poisoned());
        lock
    }

    #[test]
    fn read_unpoisoned_recovers_and_clears_the_mark() {
        let lock = poisoned();
        assert_eq!(
            *lock.read_unpoisoned(),
            [1, 2],
            "the half-done edit is visible"
        );
        assert!(!lock.is_poisoned());
    }

    #[test]
    fn write_unpoisoned_recovers_and_clears_the_mark() {
        let lock = poisoned();
        lock.write_unpoisoned().push(3);
        assert!(!lock.is_poisoned());
        assert_eq!(*lock.read_unpoisoned(), [1, 2, 3]);
    }
}
