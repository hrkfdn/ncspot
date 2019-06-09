use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use command::{
    Command, GotoMode, MoveMode, PlaylistCommands, SeekDirection, ShiftMode, TargetMode,
};
use cursive::event::{Event, Key};
use cursive::traits::View;
use cursive::views::ViewRef;
use cursive::Cursive;
use library::Library;
use queue::{Queue, RepeatSetting};
use spotify::Spotify;
use traits::ViewExt;
use ui::layout::Layout;

pub enum CommandResult {
    Consumed(Option<String>),
    View(Box<dyn ViewExt>),
    Modal(Box<dyn View>),
    Ignored,
}

pub struct CommandManager {
    aliases: HashMap<String, String>,
    spotify: Arc<Spotify>,
    queue: Arc<Queue>,
    library: Arc<Library>,
}

impl CommandManager {
    pub fn new(spotify: Arc<Spotify>, queue: Arc<Queue>, library: Arc<Library>) -> CommandManager {
        CommandManager {
            aliases: HashMap::new(),
            spotify,
            queue,
            library,
        }
    }

    pub fn register_aliases<S: Into<String>>(&mut self, name: S, aliases: Vec<S>) {
        let name = name.into();
        for a in aliases {
            self.aliases.insert(a.into(), name.clone());
        }
    }

    pub fn register_all(&mut self) {
        self.register_aliases("quit", vec!["q", "x"]);
        self.register_aliases("playpause", vec!["pause", "toggleplay", "toggleplayback"]);
        self.register_aliases("repeat", vec!["loop"]);
    }

    fn handle_default_commands(
        &self,
        s: &mut Cursive,
        cmd: &Command,
    ) -> Result<Option<String>, String> {
        match cmd {
            Command::Quit => {
                s.quit();
                Ok(None)
            }
            Command::Stop => {
                self.queue.stop();
                Ok(None)
            }
            Command::Previous => {
                if self.spotify.get_current_progress() < Duration::from_secs(5) {
                    self.queue.previous();
                } else {
                    self.spotify.seek(0);
                }
                Ok(None)
            }
            Command::Next => {
                self.queue.next(true);
                Ok(None)
            }
            Command::Clear => {
                self.queue.clear();
                Ok(None)
            }
            Command::Playlists(mode) => {
                match mode {
                    PlaylistCommands::Update => self.library.update_playlists(),
                }
                Ok(None)
            }
            Command::TogglePlay => {
                self.queue.toggleplayback();
                Ok(None)
            }
            Command::Shuffle(mode) => {
                let mode = mode.unwrap_or_else(|| !self.queue.get_shuffle());
                self.queue.set_shuffle(mode);
                Ok(None)
            }
            Command::Repeat(mode) => {
                let mode = mode.unwrap_or_else(|| match self.queue.get_repeat() {
                    RepeatSetting::None => RepeatSetting::RepeatPlaylist,
                    RepeatSetting::RepeatPlaylist => RepeatSetting::RepeatTrack,
                    RepeatSetting::RepeatTrack => RepeatSetting::None,
                });

                self.queue.set_repeat(mode);
                Ok(None)
            }
            Command::Seek(direction) => {
                match *direction {
                    SeekDirection::Relative(rel) => self.spotify.seek_relative(rel),
                    SeekDirection::Absolute(abs) => self.spotify.seek(abs),
                }
                Ok(None)
            }
            Command::Search(_)
            | Command::Move(_, _)
            | Command::Shift(_, _)
            | Command::Play
            | Command::Queue
            | Command::Save
            | Command::Delete
            | Command::Back
            | Command::Open(_)
            | Command::Goto(_) => Ok(None),
            _ => Err("Unknown Command".into()),
        }
    }

    fn handle_callbacks(&self, s: &mut Cursive, cmd: &Command) -> Result<Option<String>, String> {
        let local = {
            let mut main: ViewRef<Layout> = s.find_id("main").unwrap();
            main.on_command(s, cmd)?
        };

        if let CommandResult::Consumed(output) = local {
            Ok(output)
        } else if let CommandResult::Modal(modal) = local {
            s.add_layer(modal);
            Ok(None)
        } else if let CommandResult::View(view) = local {
            s.call_on_id("main", move |v: &mut Layout| {
                v.push_view(view);
            });

            Ok(None)
        } else {
            self.handle_default_commands(s, cmd)
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

        kb.insert("q".into(), Command::Quit);
        kb.insert("Shift+p".into(), Command::TogglePlay);
        kb.insert(
            "Shift+r".into(),
            Command::Playlists(PlaylistCommands::Update),
        );
        kb.insert("Shift+s".into(), Command::Stop);
        kb.insert("<".into(), Command::Previous);
        kb.insert(">".into(), Command::Next);
        kb.insert("c".into(), Command::Clear);
        kb.insert(" ".into(), Command::Queue);
        kb.insert("Enter".into(), Command::Play);
        kb.insert("s".into(), Command::Save);
        kb.insert("Ctrl+s".into(), Command::SaveQueue);
        kb.insert("d".into(), Command::Delete);
        kb.insert("/".into(), Command::Focus("search".into()));
        kb.insert(".".into(), Command::Seek(SeekDirection::Relative(500)));
        kb.insert(",".into(), Command::Seek(SeekDirection::Relative(-500)));
        kb.insert("r".into(), Command::Repeat(None));
        kb.insert("z".into(), Command::Shuffle(None));
        kb.insert("x".into(), Command::Share(TargetMode::Current));
        kb.insert("Shift+x".into(), Command::Share(TargetMode::Selected));

        kb.insert("F1".into(), Command::Focus("queue".into()));
        kb.insert("F2".into(), Command::Focus("search".into()));
        kb.insert("F3".into(), Command::Focus("library".into()));
        kb.insert("Backspace".into(), Command::Back);

        kb.insert("o".into(), Command::Open(TargetMode::Selected));
        kb.insert("Shift+o".into(), Command::Open(TargetMode::Current));
        kb.insert("a".into(), Command::Goto(GotoMode::Album));
        kb.insert("A".into(), Command::Goto(GotoMode::Artist));

        kb.insert("Up".into(), Command::Move(MoveMode::Up, None));
        kb.insert("Down".into(), Command::Move(MoveMode::Down, None));
        kb.insert("Left".into(), Command::Move(MoveMode::Left, None));
        kb.insert("Right".into(), Command::Move(MoveMode::Right, None));
        kb.insert("PageUp".into(), Command::Move(MoveMode::Up, Some(5)));
        kb.insert("PageDown".into(), Command::Move(MoveMode::Down, Some(5)));
        kb.insert("k".into(), Command::Move(MoveMode::Up, None));
        kb.insert("j".into(), Command::Move(MoveMode::Down, None));
        kb.insert("h".into(), Command::Move(MoveMode::Left, None));
        kb.insert("l".into(), Command::Move(MoveMode::Right, None));

        kb.insert("Shift+Up".into(), Command::Shift(ShiftMode::Up, None));
        kb.insert("Shift+Down".into(), Command::Shift(ShiftMode::Down, None));

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
