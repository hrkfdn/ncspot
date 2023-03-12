use std::collections::HashMap;

use cursive::theme::Effect;
use cursive::utils::markup::StyledString;
use cursive::view::ViewWrapper;
use cursive::views::{ScrollView, TextView};
use cursive::Cursive;

use crate::command::{Command, MoveAmount, MoveMode};
use crate::commands::CommandResult;
use crate::config::config_path;
use crate::traits::ViewExt;
use cursive::view::scroll::Scroller;

pub struct HelpView {
    view: ScrollView<TextView>,
}

impl HelpView {
    pub fn new(bindings: HashMap<String, Vec<Command>>) -> HelpView {
        let mut text = StyledString::styled("Keybindings\n\n", Effect::Bold);

        let note = format!(
            "Custom bindings can be set in {} within the [keybindings] section.\n\n",
            config_path("config.toml").to_str().unwrap_or_default()
        );
        text.append(StyledString::styled(note, Effect::Italic));

        let mut keys: Vec<&String> = bindings.keys().collect();
        keys.sort();

        for key in keys {
            let commands = &bindings[key];
            let binding = format!(
                "{} -> {}\n",
                key,
                commands
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            text.append(binding);
        }

        HelpView {
            view: ScrollView::new(TextView::new(text)),
        }
    }
}

impl ViewWrapper for HelpView {
    wrap_impl!(self.view: ScrollView<TextView>);
}

impl ViewExt for HelpView {
    fn title(&self) -> String {
        "Help".to_string()
    }

    fn on_command(&mut self, _s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Help => Ok(CommandResult::Consumed(None)),
            Command::Move(mode, amount) => {
                let scroller = self.view.get_scroller_mut();
                let viewport = scroller.content_viewport();
                match mode {
                    MoveMode::Up => {
                        match amount {
                            MoveAmount::Extreme => {
                                self.view.scroll_to_top();
                            }
                            MoveAmount::Float(scale) => {
                                let amount = (viewport.height() as f32) * scale;
                                scroller
                                    .scroll_to_y(viewport.top().saturating_sub(amount as usize));
                            }
                            MoveAmount::Integer(amount) => scroller
                                .scroll_to_y(viewport.top().saturating_sub(*amount as usize)),
                        };
                        Ok(CommandResult::Consumed(None))
                    }
                    MoveMode::Down => {
                        match amount {
                            MoveAmount::Extreme => {
                                self.view.scroll_to_bottom();
                            }
                            MoveAmount::Float(scale) => {
                                let amount = (viewport.height() as f32) * scale;
                                scroller
                                    .scroll_to_y(viewport.bottom().saturating_add(amount as usize));
                            }
                            MoveAmount::Integer(amount) => scroller
                                .scroll_to_y(viewport.bottom().saturating_add(*amount as usize)),
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
