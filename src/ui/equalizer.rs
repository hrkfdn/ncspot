use cursive::theme::Effect;
use cursive::utils::markup::StyledString;
use cursive::view::{ViewWrapper, scroll::Scroller};
use cursive::views::{ScrollView, TextView};

use crate::audio::{EQ_BAND_NAMES, EQ_MAX_GAIN_DB, EQ_MIN_GAIN_DB};
use crate::command::{Command, MoveAmount, MoveMode};
use crate::commands::CommandResult;
use crate::spotify::Spotify;
use crate::traits::ViewExt;

pub struct EqualizerView {
    view: ScrollView<TextView>,
}

impl EqualizerView {
    pub fn new(spotify: &Spotify) -> Self {
        let state = spotify.eq_state();
        let mut text = StyledString::styled("Equalizer\n\n", Effect::Bold);
        text.append(format!(
            "Status: {}\n\n",
            if state.enabled { "on" } else { "off" }
        ));
        text.append("Commands:\n");
        text.append("  :eq on | off | toggle\n");
        text.append("  :eq preset <name>\n");
        text.append("  :eq band <name|index> <dB>\n");
        text.append("  :eq band bass +1\n");
        text.append("  :eq reset\n\n");
        text.append(format!(
            "Gain range: {EQ_MIN_GAIN_DB} .. {EQ_MAX_GAIN_DB} dB\n\n"
        ));
        for (i, gain) in state.bands.iter().enumerate() {
            text.append(format!(
                "{:>2} {:>10} {:>5.1} dB\n",
                i, EQ_BAND_NAMES[i], gain
            ));
        }
        Self {
            view: ScrollView::new(TextView::new(text)),
        }
    }
}

impl ViewWrapper for EqualizerView {
    wrap_impl!(self.view: ScrollView<TextView>);
}

impl ViewExt for EqualizerView {
    fn title(&self) -> String {
        "Equalizer".to_string()
    }

    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::EqualizerView => Ok(CommandResult::Consumed(None)),
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
                                let scroll = (viewport.height() as f32) * scale;
                                scroller
                                    .scroll_to_y(viewport.top().saturating_sub(scroll as usize));
                            }
                            MoveAmount::Integer(n) => {
                                scroller.scroll_to_y(viewport.top().saturating_sub(*n as usize));
                            }
                        };
                        Ok(CommandResult::Consumed(None))
                    }
                    MoveMode::Down => {
                        match amount {
                            MoveAmount::Extreme => {
                                self.view.scroll_to_bottom();
                            }
                            MoveAmount::Float(scale) => {
                                let scroll = (viewport.height() as f32) * scale;
                                scroller
                                    .scroll_to_y(viewport.bottom().saturating_add(scroll as usize));
                            }
                            MoveAmount::Integer(n) => {
                                scroller.scroll_to_y(viewport.bottom().saturating_add(*n as usize));
                            }
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
