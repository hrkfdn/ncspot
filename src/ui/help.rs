use std::collections::HashMap;

use cursive::theme::Effect;
use cursive::utils::markup::StyledString;
use cursive::view::ViewWrapper;
use cursive::views::{ScrollView, TextView};

use crate::command::Command;
use crate::config::config_path;
use crate::traits::ViewExt;

pub struct HelpView {
    view: ScrollView<TextView>,
}

impl HelpView {
    pub fn new(bindings: HashMap<String, Command>) -> HelpView {
        let mut text = StyledString::styled("Keybindings\n\n", Effect::Bold);

        let note = format!(
            "Custom bindings can be set in {} within the [keybindings] section.\n\n",
            config_path("config.toml").to_str().unwrap_or_default()
        );
        text.append(StyledString::styled(note, Effect::Italic));

        let mut keys: Vec<&String> = bindings.keys().collect();
        keys.sort();

        for key in keys {
            let command = &bindings[key];
            let binding = format!("{} -> {}\n", key, command);
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
}
