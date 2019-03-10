use std::collections::HashMap;

use cursive::Cursive;

pub struct CommandManager {
    commands: HashMap<String, Box<dyn Fn(&mut Cursive, Vec<String>) -> Result<Option<String>, String>>>,
    aliases: HashMap<String, String>,
}

impl CommandManager {
    pub fn new() -> CommandManager {
        CommandManager {
            commands: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn register<S: Into<String>>(
        &mut self,
        name: S,
        aliases: Vec<S>,
        cb: Box<dyn Fn(&mut Cursive, Vec<String>) -> Result<Option<String>, String>>
    ) {
        let name = name.into();
        for a in aliases {
            self.aliases.insert(a.into(), name.clone());
        }
        self.commands.insert(name, cb);
    }

    fn handle_aliases(&self, name: &String) -> String {
        if let Some(s) = self.aliases.get(name) {
            self.handle_aliases(s)
        } else {
            name.clone()
        }
    }

    pub fn handle(&self, s: &mut Cursive, cmd: String) -> Result<Option<String>, String> {
        let components: Vec<String> = cmd.split(' ').map(|s| s.to_string()).collect();

        if let Some(cb) = self.commands.get(&self.handle_aliases(&components[0])) {
            cb(s, components[1..].to_vec())
        } else {
            Err("Unknown command.".to_string())
        }
    }
}
