use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cursive::event::{Event, Key};
use cursive::views::ViewRef;
use cursive::Cursive;

use clipboard::{ClipboardContext, ClipboardProvider};
use library::Library;
use queue::{Queue, RepeatSetting};
use spotify::Spotify;
use traits::{ListItem, ViewExt};
use ui::layout::Layout;

type CommandCb = dyn Fn(&mut Cursive, &[String]) -> Result<Option<String>, String>;

pub enum CommandResult {
    Consumed(Option<String>),
    View(Box<dyn ViewExt>),
    Ignored,
}

pub struct CommandManager {
    callbacks: HashMap<String, Option<Box<CommandCb>>>,
    aliases: HashMap<String, String>,
}

impl CommandManager {
    pub fn new() -> CommandManager {
        CommandManager {
            callbacks: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn register_command<S: Into<String>>(&mut self, name: S, cb: Option<Box<CommandCb>>) {
        self.callbacks.insert(name.into(), cb);
    }

    pub fn register_aliases<S: Into<String>>(&mut self, name: S, aliases: Vec<S>) {
        let name = name.into();
        for a in aliases {
            self.aliases.insert(a.into(), name.clone());
        }
    }

    pub fn register_all(
        &mut self,
        spotify: Arc<Spotify>,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) {
        self.register_aliases("quit", vec!["q", "x"]);
        self.register_aliases("playpause", vec!["pause", "toggleplay", "toggleplayback"]);
        self.register_aliases("repeat", vec!["loop"]);

        self.register_command("search", None);
        self.register_command("move", None);
        self.register_command("shift", None);
        self.register_command("play", None);
        self.register_command("queue", None);
        self.register_command("save", None);
        self.register_command("delete", None);
        self.register_command("back", None);
        self.register_command("open", None);
        self.register_command("goto", None);

        self.register_command(
            "quit",
            Some(Box::new(move |s, _args| {
                s.quit();
                Ok(None)
            })),
        );

        {
            let queue = queue.clone();
            self.register_command(
                "stop",
                Some(Box::new(move |_s, _args| {
                    queue.stop();
                    Ok(None)
                })),
            );
        }

        {
            let queue = queue.clone();
            let spotify = spotify.clone();
            self.register_command(
                "previous",
                Some(Box::new(move |_s, _args| {
                    if spotify.get_current_progress() < Duration::from_secs(5) {
                        queue.previous();
                    } else {
                        spotify.seek(0);
                    }
                    Ok(None)
                })),
            );
        }

        {
            let queue = queue.clone();
            self.register_command(
                "next",
                Some(Box::new(move |_s, _args| {
                    queue.next(true);
                    Ok(None)
                })),
            );
        }

        {
            let queue = queue.clone();
            self.register_command(
                "clear",
                Some(Box::new(move |_s, _args| {
                    queue.clear();
                    Ok(None)
                })),
            );
        }

        {
            let library = library.clone();
            self.register_command(
                "playlists",
                Some(Box::new(move |_s, args| {
                    if let Some(arg) = args.get(0) {
                        if arg == "update" {
                            library.update_playlists();
                        }
                    }
                    Ok(None)
                })),
            );
        }

        {
            let queue = queue.clone();
            self.register_command(
                "playpause",
                Some(Box::new(move |_s, _args| {
                    queue.toggleplayback();
                    Ok(None)
                })),
            );
        }

        {
            let queue = queue.clone();
            self.register_command(
                "shuffle",
                Some(Box::new(move |_s, args| {
                    if let Some(arg) = args.get(0) {
                        queue.set_shuffle(match arg.as_ref() {
                            "on" => true,
                            "off" => false,
                            _ => {
                                return Err("Unknown shuffle setting.".to_string());
                            }
                        });
                    } else {
                        queue.set_shuffle(!queue.get_shuffle());
                    }

                    Ok(None)
                })),
            );
        }

        {
            let queue = queue.clone();
            self.register_command(
                "repeat",
                Some(Box::new(move |_s, args| {
                    if let Some(arg) = args.get(0) {
                        queue.set_repeat(match arg.as_ref() {
                            "list" | "playlist" | "queue" => RepeatSetting::RepeatPlaylist,
                            "track" | "once" => RepeatSetting::RepeatTrack,
                            "none" | "off" => RepeatSetting::None,
                            _ => {
                                return Err("Unknown loop setting.".to_string());
                            }
                        });
                    } else {
                        queue.set_repeat(match queue.get_repeat() {
                            RepeatSetting::None => RepeatSetting::RepeatPlaylist,
                            RepeatSetting::RepeatPlaylist => RepeatSetting::RepeatTrack,
                            RepeatSetting::RepeatTrack => RepeatSetting::None,
                        });
                    }

                    Ok(None)
                })),
            );
        }

        {
            let spotify = spotify.clone();
            self.register_command(
                "seek",
                Some(Box::new(move |_s, args| {
                    if let Some(arg) = args.get(0) {
                        match arg.chars().next().unwrap() {
                            '+' | '-' => {
                                spotify.seek_relative(arg.parse::<i32>().unwrap_or(0));
                            }
                            _ => {
                                spotify.seek(arg.parse::<u32>().unwrap_or(0));
                            }
                        }
                    }

                    Ok(None)
                })),
            );
        }
    }

    fn handle_aliases(&self, name: &str) -> String {
        if let Some(s) = self.aliases.get(name) {
            self.handle_aliases(s)
        } else {
            name.to_string()
        }
    }

    fn handle_callbacks(
        &self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<Option<String>, String> {
        let local = {
            let mut main: ViewRef<Layout> = s.find_id("main").unwrap();
            main.on_command(s, cmd, args)?
        };

        if let CommandResult::Consumed(output) = local {
            Ok(output)
        } else if let CommandResult::View(view) = local {
            s.call_on_id("main", move |v: &mut Layout| {
                v.push_view(view);
            });

            Ok(None)
        } else if let Some(callback) = self.callbacks.get(cmd) {
            callback.as_ref().map(|cb| cb(s, args)).unwrap_or(Ok(None))
        } else {
            Err("Unknown command.".to_string())
        }
    }

    pub fn handle(&self, s: &mut Cursive, cmd: String) {
        let components: Vec<String> = cmd
            .trim()
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();

        let cmd = self.handle_aliases(&components[0]);
        let args = components[1..].to_vec();

        let result = self.handle_callbacks(s, &cmd, &args);

        s.call_on_id("main", |v: &mut Layout| {
            v.set_result(result);
        });

        s.on_event(Event::Refresh);
    }

    pub fn register_keybinding<E: Into<cursive::event::Event>, S: Into<String>>(
        this: Arc<Self>,
        cursive: &mut Cursive,
        event: E,
        command: S,
    ) {
        let cmd = command.into();
        cursive.add_global_callback(event, move |s| {
            this.handle(s, cmd.clone());
        });
    }

    pub fn register_keybindings(
        this: Arc<Self>,
        cursive: &mut Cursive,
        keybindings: Option<HashMap<String, String>>,
    ) {
        let mut kb = Self::default_keybindings();
        kb.extend(keybindings.unwrap_or_default());

        for (k, v) in kb {
            if let Some(binding) = Self::parse_keybinding(&k) {
                Self::register_keybinding(this.clone(), cursive, binding, v);
            } else {
                error!("Could not parse keybinding: \"{}\"", &k);
            }
        }
    }

    fn default_keybindings() -> HashMap<String, String> {
        let mut kb = HashMap::new();

        kb.insert("q".into(), "quit".into());
        kb.insert("P".into(), "toggleplay".into());
        kb.insert("R".into(), "playlists update".into());
        kb.insert("S".into(), "stop".into());
        kb.insert("<".into(), "previous".into());
        kb.insert(">".into(), "next".into());
        kb.insert("c".into(), "clear".into());
        kb.insert(" ".into(), "queue".into());
        kb.insert("Enter".into(), "play".into());
        kb.insert("s".into(), "save".into());
        kb.insert("Ctrl+s".into(), "save queue".into());
        kb.insert("d".into(), "delete".into());
        kb.insert("/".into(), "focus search".into());
        kb.insert(".".into(), "seek +500".into());
        kb.insert(",".into(), "seek -500".into());
        kb.insert("r".into(), "repeat".into());
        kb.insert("z".into(), "shuffle".into());
        kb.insert("x".into(), "share current".into());
        kb.insert("Shift+x".into(), "share selected".into());

        kb.insert("F1".into(), "focus queue".into());
        kb.insert("F2".into(), "focus search".into());
        kb.insert("F3".into(), "focus library".into());
        kb.insert("Backspace".into(), "back".into());

        kb.insert("o".into(), "open".into());
        kb.insert("a".into(), "goto album".into());
        kb.insert("A".into(), "goto artist".into());

        kb.insert("Up".into(), "move up".into());
        kb.insert("Down".into(), "move down".into());
        kb.insert("Left".into(), "move left".into());
        kb.insert("Right".into(), "move right".into());
        kb.insert("PageUp".into(), "move up 5".into());
        kb.insert("PageDown".into(), "move down 5".into());
        kb.insert("k".into(), "move up".into());
        kb.insert("j".into(), "move down".into());
        kb.insert("h".into(), "move left".into());
        kb.insert("l".into(), "move right".into());

        kb.insert("Shift+Up".into(), "shift up".into());
        kb.insert("Shift+Down".into(), "shift down".into());

        kb
    }

    fn parse_key(key: &str) -> Event {
        match key {
            "Enter" => Event::Key(Key::Enter),
            "Tab" => Event::Key(Key::Tab),
            "Backspace" => Event::Key(Key::Backspace),
            "Esc" => Event::Key(Key::Esc),
            "Left" => Event::Key(Key::Left),
            "Right" => Event::Key(Key::Right),
            "Up" => Event::Key(Key::Up),
            "Down" => Event::Key(Key::Down),
            "Ins" => Event::Key(Key::Ins),
            "Del" => Event::Key(Key::Del),
            "Home" => Event::Key(Key::Home),
            "End" => Event::Key(Key::End),
            "PageUp" => Event::Key(Key::PageUp),
            "PageDown" => Event::Key(Key::PageDown),
            "PauseBreak" => Event::Key(Key::PauseBreak),
            "NumpadCenter" => Event::Key(Key::NumpadCenter),
            "F0" => Event::Key(Key::F0),
            "F1" => Event::Key(Key::F1),
            "F2" => Event::Key(Key::F2),
            "F3" => Event::Key(Key::F3),
            "F4" => Event::Key(Key::F4),
            "F5" => Event::Key(Key::F5),
            "F6" => Event::Key(Key::F6),
            "F7" => Event::Key(Key::F7),
            "F8" => Event::Key(Key::F8),
            "F9" => Event::Key(Key::F9),
            "F10" => Event::Key(Key::F10),
            "F11" => Event::Key(Key::F11),
            "F12" => Event::Key(Key::F12),
            s => Event::Char(s.chars().next().unwrap()),
        }
    }

    fn parse_keybinding(kb: &str) -> Option<cursive::event::Event> {
        let mut split = kb.split('+');
        if split.clone().count() == 2 {
            let modifier = split.next().unwrap();
            let key = split.next().unwrap();
            let parsed = Self::parse_key(key);
            if let Event::Key(parsed) = parsed {
                match modifier {
                    "Shift" => Some(Event::Shift(parsed)),
                    "Alt" => Some(Event::Alt(parsed)),
                    "Ctrl" => Some(Event::Ctrl(parsed)),
                    _ => None,
                }
            } else if let Event::Char(parsed) = parsed {
                match modifier {
                    "Shift" => Some(Event::Char(parsed.to_uppercase().next().unwrap())),
                    "Alt" => Some(Event::AltChar(parsed)),
                    "Ctrl" => Some(Event::CtrlChar(parsed)),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            Some(Self::parse_key(&kb))
        }
    }
}
