use crate::queue::RepeatSetting;
use std::collections::HashMap;
use std::fmt;
use std::iter::FromIterator;

use strum_macros::Display;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SeekInterval {
    Forward,
    Backwards,
    Custom(usize),
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum TargetMode {
    Current,
    Selected,
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum MoveMode {
    Up,
    Down,
    Left,
    Right,
    Playing,
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum MoveAmount {
    Integer(i32),
    Extreme,
}

impl Default for MoveAmount {
    fn default() -> Self {
        MoveAmount::Integer(1)
    }
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum ShiftMode {
    Up,
    Down,
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum GotoMode {
    Album,
    Artist,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SeekDirection {
    Relative(i32),
    Absolute(u32),
}

impl fmt::Display for SeekDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            SeekDirection::Absolute(pos) => format!("{}", pos),
            SeekDirection::Relative(delta) => {
                format!("{}{}", if delta > &0 { "+" } else { "" }, delta)
            }
        };
        write!(f, "{}", repr)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Command {
    Quit,
    TogglePlay,
    Stop,
    Previous,
    Next,
    Clear,
    Queue,
    PlayNext,
    Play,
    UpdateLibrary,
    Save,
    SaveQueue,
    Delete,
    Focus(String),
    Seek(SeekDirection),
    VolumeUp,
    VolumeDown,
    Repeat(Option<RepeatSetting>),
    Shuffle(Option<bool>),
    Share(TargetMode),
    Back,
    Open(TargetMode),
    Goto(GotoMode),
    Move(MoveMode, MoveAmount),
    Shift(ShiftMode, Option<i32>),
    Search(String),
    Jump(String),
    Help,
    ReloadConfig,
    Noop,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Command::Noop => "noop".to_string(),
            Command::Quit => "quit".to_string(),
            Command::TogglePlay => "playpause".to_string(),
            Command::Stop => "stop".to_string(),
            Command::Previous => "previous".to_string(),
            Command::Next => "next".to_string(),
            Command::Clear => "clear".to_string(),
            Command::Queue => "queue".to_string(),
            Command::PlayNext => "play next".to_string(),
            Command::Play => "play".to_string(),
            Command::UpdateLibrary => "update".to_string(),
            Command::Save => "save".to_string(),
            Command::SaveQueue => "save queue".to_string(),
            Command::Delete => "delete".to_string(),
            Command::Focus(tab) => format!("focus {}", tab),
            Command::Seek(direction) => format!("seek {}", direction),
            Command::VolumeUp => "volup".to_string(),
            Command::VolumeDown => "voldown".to_string(),
            Command::Repeat(mode) => {
                let param = match mode {
                    Some(mode) => format!("{}", mode),
                    None => "".to_string(),
                };
                format!("repeat {}", param)
            }
            Command::Shuffle(on) => {
                let param = on.map(|x| if x { "on" } else { "off" });
                format!("shuffle {}", param.unwrap_or(""))
            }
            Command::Share(mode) => format!("share {}", mode),
            Command::Back => "back".to_string(),
            Command::Open(mode) => format!("open {}", mode),
            Command::Goto(mode) => format!("goto {}", mode),
            Command::Move(mode, MoveAmount::Extreme) => format!(
                "move {}",
                match mode {
                    MoveMode::Up => "top",
                    MoveMode::Down => "bottom",
                    MoveMode::Left => "leftmost",
                    MoveMode::Right => "rightmost",
                    _ => "",
                }
            ),
            Command::Move(MoveMode::Playing, _) => "move playing".to_string(),
            Command::Move(mode, MoveAmount::Integer(amount)) => format!("move {} {}", mode, amount),
            Command::Shift(mode, amount) => format!("shift {} {}", mode, amount.unwrap_or(1)),
            Command::Search(term) => format!("search {}", term),
            Command::Jump(term) => format!("jump {}", term),
            Command::Help => "help".to_string(),
            Command::ReloadConfig => "reload".to_string(),
        };
        write!(f, "{}", repr)
    }
}

fn register_aliases(map: &mut HashMap<&str, &str>, cmd: &'static str, names: Vec<&'static str>) {
    for a in names {
        map.insert(a, cmd);
    }
}

lazy_static! {
    static ref ALIASES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();

        register_aliases(&mut m, "quit", vec!["q", "x"]);
        register_aliases(
            &mut m,
            "playpause",
            vec!["pause", "toggleplay", "toggleplayback"],
        );
        register_aliases(&mut m, "repeat", vec!["loop"]);

        m.insert("1", "foo");
        m.insert("2", "bar");
        m.insert("3", "baz");
        m
    };
}

fn handle_aliases(input: &str) -> &str {
    if let Some(cmd) = ALIASES.get(input) {
        handle_aliases(cmd)
    } else {
        input
    }
}

pub fn parse(input: &str) -> Option<Command> {
    let components: Vec<_> = input.trim().split(' ').collect();

    let command = handle_aliases(&components[0]);
    let args = components[1..].to_vec();

    match command {
        "quit" => Some(Command::Quit),
        "playpause" => Some(Command::TogglePlay),
        "stop" => Some(Command::Stop),
        "previous" => Some(Command::Previous),
        "next" => Some(Command::Next),
        "clear" => Some(Command::Clear),
        "playnext" => Some(Command::PlayNext),
        "queue" => Some(Command::Queue),
        "play" => Some(Command::Play),
        "update" => Some(Command::UpdateLibrary),
        "delete" => Some(Command::Delete),
        "back" => Some(Command::Back),
        "open" => args
            .get(0)
            .and_then(|target| match *target {
                "selected" => Some(TargetMode::Selected),
                "current" => Some(TargetMode::Current),
                _ => None,
            })
            .map(Command::Open),
        "jump" => Some(Command::Jump(args.join(" "))),
        "search" => args
            .get(0)
            .map(|query| Command::Search((*query).to_string())),
        "shift" => {
            let amount = args.get(1).and_then(|amount| amount.parse().ok());

            args.get(0)
                .and_then(|direction| match *direction {
                    "up" => Some(ShiftMode::Up),
                    "down" => Some(ShiftMode::Down),
                    _ => None,
                })
                .map(|mode| Command::Shift(mode, amount))
        }
        "move" => {
            let cmd: Option<Command> = {
                args.get(0).and_then(|extreme| match *extreme {
                    "top" => Some(Command::Move(MoveMode::Up, MoveAmount::Extreme)),
                    "bottom" => Some(Command::Move(MoveMode::Down, MoveAmount::Extreme)),
                    "leftmost" => Some(Command::Move(MoveMode::Left, MoveAmount::Extreme)),
                    "rightmost" => Some(Command::Move(MoveMode::Right, MoveAmount::Extreme)),
                    "playing" => Some(Command::Move(MoveMode::Playing, MoveAmount::default())),
                    _ => None,
                })
            };

            cmd.or({
                let amount = args
                    .get(1)
                    .and_then(|amount| amount.parse().ok())
                    .map(MoveAmount::Integer)
                    .unwrap_or_default();

                args.get(0)
                    .and_then(|direction| match *direction {
                        "up" => Some(MoveMode::Up),
                        "down" => Some(MoveMode::Down),
                        "left" => Some(MoveMode::Left),
                        "right" => Some(MoveMode::Right),
                        _ => None,
                    })
                    .map(|mode| Command::Move(mode, amount))
            })
        }
        "goto" => args
            .get(0)
            .and_then(|mode| match *mode {
                "album" => Some(GotoMode::Album),
                "artist" => Some(GotoMode::Artist),
                _ => None,
            })
            .map(Command::Goto),
        "share" => args
            .get(0)
            .and_then(|target| match *target {
                "selected" => Some(TargetMode::Selected),
                "current" => Some(TargetMode::Current),
                _ => None,
            })
            .map(Command::Share),
        "shuffle" => {
            let shuffle = args.get(0).and_then(|mode| match *mode {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            });

            Some(Command::Shuffle(shuffle))
        }
        "repeat" => {
            let mode = args.get(0).and_then(|mode| match *mode {
                "list" | "playlist" | "queue" => Some(RepeatSetting::RepeatPlaylist),
                "track" | "once" => Some(RepeatSetting::RepeatTrack),
                "none" | "off" => Some(RepeatSetting::None),
                _ => None,
            });

            Some(Command::Repeat(mode))
        }
        "seek" => args.get(0).and_then(|arg| match arg.chars().next() {
            Some(x) if x == '-' || x == '+' => String::from_iter(arg.chars().skip(1))
                .parse::<i32>()
                .ok()
                .map(|amount| {
                    Command::Seek(SeekDirection::Relative(
                        amount
                            * match x {
                                '-' => -1,
                                _ => 1,
                            },
                    ))
                }),
            _ => String::from_iter(arg.chars())
                .parse()
                .ok()
                .map(|amount| Command::Seek(SeekDirection::Absolute(amount))),
        }),
        "focus" => args
            .get(0)
            .map(|target| Command::Focus((*target).to_string())),
        "save" => args
            .get(0)
            .map(|target| match *target {
                "queue" => Command::SaveQueue,
                _ => Command::Save,
            })
            .or(Some(Command::Save)),
        "volup" => Some(Command::VolumeUp),
        "voldown" => Some(Command::VolumeDown),
        "help" => Some(Command::Help),
        "reload" => Some(Command::ReloadConfig),
        "noop" => Some(Command::Noop),
        _ => None,
    }
}
