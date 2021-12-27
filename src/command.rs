use crate::queue::RepeatSetting;
use std::collections::HashMap;
use std::fmt;

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
pub enum SortKey {
    Title,
    Duration,
    Artist,
    Album,
    Added,
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Display, Clone, Serialize, Deserialize, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum JumpMode {
    Previous,
    Next,
    Query(String),
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
    VolumeUp(u16),
    VolumeDown(u16),
    Repeat(Option<RepeatSetting>),
    Shuffle(Option<bool>),
    Share(TargetMode),
    Back,
    Open(TargetMode),
    Goto(GotoMode),
    Move(MoveMode, MoveAmount),
    Shift(ShiftMode, Option<i32>),
    Search(String),
    Jump(JumpMode),
    Help,
    ReloadConfig,
    Noop,
    Insert(Option<String>),
    NewPlaylist(String),
    Sort(SortKey, SortDirection),
    Logout,
    ShowRecommendations(TargetMode),
    Redraw,
    Execute(String),
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
            Command::PlayNext => "playnext".to_string(),
            Command::Play => "play".to_string(),
            Command::UpdateLibrary => "update".to_string(),
            Command::Save => "save".to_string(),
            Command::SaveQueue => "save queue".to_string(),
            Command::Delete => "delete".to_string(),
            Command::Focus(tab) => format!("focus {}", tab),
            Command::Seek(direction) => format!("seek {}", direction),
            Command::VolumeUp(amount) => format!("volup {}", amount),
            Command::VolumeDown(amount) => format!("voldown {}", amount),
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
            Command::Jump(mode) => match mode {
                JumpMode::Previous => "jumpprevious".to_string(),
                JumpMode::Next => "jumpnext".to_string(),
                JumpMode::Query(term) => String::from(format!("jump {}", term)),
            },
            Command::Help => "help".to_string(),
            Command::ReloadConfig => "reload".to_string(),
            Command::Insert(_) => "insert".to_string(),
            Command::NewPlaylist(name) => format!("new playlist {}", name),
            Command::Sort(key, direction) => format!("sort {} {}", key, direction),
            Command::Logout => "logout".to_string(),
            Command::ShowRecommendations(mode) => format!("similar {}", mode),
            Command::Redraw => "redraw".to_string(),
            Command::Execute(cmd) => format!("exec {}", cmd),
        };
        // escape the command separator
        let repr = repr.replace(";", ";;");
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

pub fn parse(input: &str) -> Option<Vec<Command>> {
    let mut command_inputs = vec!["".to_string()];
    let mut command_idx = 0;
    enum ParseState {
        Normal,
        SeparatorEncountered,
    }
    let mut parse_state = ParseState::Normal;
    for c in input.chars() {
        let is_separator = c == ';';
        match parse_state {
            ParseState::Normal if is_separator => parse_state = ParseState::SeparatorEncountered,
            ParseState::Normal => command_inputs[command_idx].push(c),
            // ";" is escaped using ";;", so if the previous char already was a ';' push a ';'.
            ParseState::SeparatorEncountered if is_separator => {
                command_inputs[command_idx].push(c);
                parse_state = ParseState::Normal;
            }
            ParseState::SeparatorEncountered => {
                command_idx += 1;
                command_inputs.push(c.to_string());
                parse_state = ParseState::Normal;
            }
        }
    }

    let mut commands = vec![];
    for command_input in command_inputs {
        let components: Vec<_> = command_input.trim().split(' ').collect();

        let command = handle_aliases(components[0]);
        let args = components[1..].to_vec();

        let command = match command {
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
            "jump" => Some(Command::Jump(JumpMode::Query(args.join(" ")))),
            "jumpnext" => Some(Command::Jump(JumpMode::Next)),
            "jumpprevious" => Some(Command::Jump(JumpMode::Previous)),
            "search" => Some(Command::Search(args.join(" "))),
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
                    "track" | "once" | "single" => Some(RepeatSetting::RepeatTrack),
                    "none" | "off" => Some(RepeatSetting::None),
                    _ => None,
                });

                Some(Command::Repeat(mode))
            }
            "seek" => {
                let arg = args.join(" ");
                let first_char = arg.chars().next();
                let duration_raw = match first_char {
                    Some('+' | '-') => arg.chars().skip(1).collect(),
                    _ => arg.to_string(),
                };
                duration_raw
                    .parse::<u32>() // accept raw milliseconds for backward compatibility
                    .ok()
                    .or_else(|| {
                        parse_duration::parse(&duration_raw) // accept fancy duration
                            .ok()
                            .and_then(|dur| dur.as_millis().try_into().ok())
                    })
                    .and_then(|unsigned_millis| {
                        match first_char {
                            // handle i32::MAX < unsigned_millis < u32::MAX gracefully
                            Some('+') => {
                                i32::try_from(unsigned_millis)
                                    .ok()
                                    .map(|unsigned_millis_i32| {
                                        SeekDirection::Relative(unsigned_millis_i32)
                                    })
                            }
                            Some('-') => {
                                i32::try_from(unsigned_millis)
                                    .ok()
                                    .map(|unsigned_millis_i32| {
                                        SeekDirection::Relative(-unsigned_millis_i32)
                                    })
                            }
                            _ => Some(SeekDirection::Absolute(unsigned_millis)),
                        }
                        .map(|direction| Command::Seek(direction))
                    })
            }
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
            "volup" => Some(Command::VolumeUp(
                args.get(0).and_then(|v| v.parse::<u16>().ok()).unwrap_or(1),
            )),
            "voldown" => Some(Command::VolumeDown(
                args.get(0).and_then(|v| v.parse::<u16>().ok()).unwrap_or(1),
            )),
            "help" => Some(Command::Help),
            "reload" => Some(Command::ReloadConfig),
            "insert" => {
                if args.is_empty() {
                    Some(Command::Insert(None))
                } else {
                    args.get(0)
                        .map(|url| Command::Insert(Some((*url).to_string())))
                }
            }
            "newplaylist" => {
                if !args.is_empty() {
                    Some(Command::NewPlaylist(args.join(" ")))
                } else {
                    None
                }
            }
            "sort" => {
                if !args.is_empty() {
                    let sort_key = args.get(0).and_then(|key| match *key {
                        "title" => Some(SortKey::Title),
                        "duration" => Some(SortKey::Duration),
                        "album" => Some(SortKey::Album),
                        "added" => Some(SortKey::Added),
                        "artist" => Some(SortKey::Artist),
                        _ => None,
                    })?;

                    let sort_direction = args
                        .get(1)
                        .map(|direction| match *direction {
                            "a" => SortDirection::Ascending,
                            "asc" => SortDirection::Ascending,
                            "ascending" => SortDirection::Ascending,
                            "d" => SortDirection::Descending,
                            "desc" => SortDirection::Descending,
                            "descending" => SortDirection::Descending,
                            _ => SortDirection::Ascending,
                        })
                        .unwrap_or(SortDirection::Ascending);

                    Some(Command::Sort(sort_key, sort_direction))
                } else {
                    None
                }
            }
            "logout" => Some(Command::Logout),
            "similar" => args
                .get(0)
                .and_then(|target| match *target {
                    "selected" => Some(TargetMode::Selected),
                    "current" => Some(TargetMode::Current),
                    _ => None,
                })
                .map(Command::ShowRecommendations),
            "noop" => Some(Command::Noop),
            "redraw" => Some(Command::Redraw),
            "exec" => Some(Command::Execute(args.join(" "))),
            _ => None,
        };
        commands.push(command?);
    }
    Some(commands)
}
