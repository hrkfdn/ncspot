use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cursive::event::{Event, Key};
use cursive::views::ViewRef;
use cursive::Cursive;

use command::Command::{
    Back, Clear, Delete, Focus, Goto, Move, Next, Open, Play, Playlists, Previous, Quit, Repeat,
    Save, SaveQueue, Seek, Share, Shift, Shuffle, Stop, TogglePlay,
};
use command::GotoMode::{Album, Artist};
use command::MoveMode::{Down, Left, Right, Up};
use command::PlaylistCommands::Update;
use command::SeekInterval::{Backwards, Forward};
use command::TargetMode::{Current, Selected};
use command::{Command, ShiftMode};
use library::Library;
use queue::{Queue, RepeatSetting};
use spotify::Spotify;
use traits::ViewExt;
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

    fn handle_callbacks(&self, s: &mut Cursive, cmd: &Command) -> Result<Option<String>, String> {
        let local = {
            let mut main: ViewRef<Layout> = s.find_id("main").unwrap();
            main.on_command(s, cmd)?
        };

        if let CommandResult::Consumed(output) = local {
            Ok(output)
        } else if let CommandResult::View(view) = local {
            s.call_on_id("main", move |v: &mut Layout| {
                v.push_view(view);
            });

            Ok(None)
        }
        /* handle default commands
        else if let Some(callback) = self.callbacks.get(cmd) {
            callback.as_ref().map(|cb| cb(s, args)).unwrap_or(Ok(None))
        } */
        else {
            Err("Unknown command.".to_string())
        }
    }

    pub fn handle(&self, s: &mut Cursive, cmd: Command) {
        let result = self.handle_callbacks(s, &cmd);

        s.call_on_id("main", |v: &mut Layout| {
            v.set_result(result);
        });

        s.on_event(Event::Refresh);
    }

    pub fn register_keybinding<E: Into<cursive::event::Event>>(
        this: Arc<Self>,
        cursive: &mut Cursive,
        event: E,
        command: Command,
    ) {
        cursive.add_global_callback(event, move |s| {
            this.handle(s, command.clone());
        });
    }

    pub fn register_keybindings(
        this: Arc<Self>,
        cursive: &mut Cursive,
        keybindings: Option<HashMap<String, Command>>,
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

    fn default_keybindings() -> HashMap<String, Command> {
        let mut kb = HashMap::new();

        kb.insert("q".into(), Quit);
        kb.insert("P".into(), TogglePlay);
        kb.insert("R".into(), Playlists(Update));
        kb.insert("S".into(), Stop);
        kb.insert("<".into(), Previous);
        kb.insert(">".into(), Next);
        kb.insert("c".into(), Clear);
        kb.insert(" ".into(), Command::Queue);
        kb.insert("Enter".into(), Play);
        kb.insert("s".into(), Save);
        kb.insert("Ctrl+s".into(), SaveQueue);
        kb.insert("d".into(), Delete);
        kb.insert("/".into(), Focus("search".into()));
        kb.insert(".".into(), Seek(Forward));
        kb.insert(",".into(), Seek(Backwards));
        kb.insert("r".into(), Repeat);
        kb.insert("z".into(), Shuffle);
        kb.insert("x".into(), Share(Current));
        kb.insert("Shift+x".into(), Share(Selected));

        kb.insert("F1".into(), Focus("queue".into()));
        kb.insert("F2".into(), Focus("search".into()));
        kb.insert("F3".into(), Focus("library".into()));
        kb.insert("Backspace".into(), Back);

        kb.insert("o".into(), Open);
        kb.insert("a".into(), Goto(Album));
        kb.insert("A".into(), Goto(Artist));

        kb.insert("Up".into(), Move(Up, None));
        kb.insert("Down".into(), Move(Down, None));
        kb.insert("Left".into(), Move(Left, None));
        kb.insert("Right".into(), Move(Right, None));
        kb.insert("PageUp".into(), Move(Up, Some(5)));
        kb.insert("PageDown".into(), Move(Down, Some(5)));
        kb.insert("k".into(), Move(Up, None));
        kb.insert("j".into(), Move(Down, None));
        kb.insert("h".into(), Move(Left, None));
        kb.insert("l".into(), Move(Right, None));

        kb.insert("Shift+Up".into(), Shift(ShiftMode::Up, None));
        kb.insert("Shift+Down".into(), Shift(ShiftMode::Down, None));

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
