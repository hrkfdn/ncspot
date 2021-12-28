use crate::queue::RepeatSetting;
use std::collections::HashMap;
use std::fmt;

use regex::Regex;
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommandParseError {
    NoSuchCommand { cmd: String },
    InsufficientArgs { cmd: String, hint: Option<String> },
    BadEnumArg { arg: String, accept: Vec<String> },
    ArgParseError { arg: String, err: String },
}

impl fmt::Display for CommandParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CommandParseError::*;
        let formatted = match self {
            NoSuchCommand { cmd } => format!("No such command \"{}\"", cmd),
            InsufficientArgs { cmd, hint } => {
                if let Some(hint_str) = hint {
                    format!("\"{}\" requires additional arguments: {}", cmd, hint_str)
                } else {
                    format!("\"{}\" requires additional arguments", cmd)
                }
            }
            BadEnumArg { arg, accept } => {
                format!(
                    "Illegal argument \"{}\": supported values are {}",
                    arg,
                    accept.join("|")
                )
            }
            ArgParseError { arg, err } => format!("Error with argument \"{}\": {}", arg, err),
        };
        write!(f, "{}", formatted)
    }
}

pub fn parse(input: &str) -> Result<Vec<Command>, CommandParseError> {
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
        lazy_static! {
            // https://docs.rs/regex/latest/regex/#example-avoid-compiling-the-same-regex-in-a-loop
            static ref CONTINUOUS_SPACE_MATCHER: Regex = Regex::new(r" +").unwrap();
        }
        let command_sanitised = CONTINUOUS_SPACE_MATCHER.replace_all(&command_input, " ");
        let components: Vec<_> = command_sanitised.trim().split(' ').collect();

        let command = handle_aliases(components[0]);
        let args = components[1..].to_vec();

        use CommandParseError::*;
        let command = match command {
            "quit" => Command::Quit,
            "playpause" => Command::TogglePlay,
            "stop" => Command::Stop,
            "previous" => Command::Previous,
            "next" => Command::Next,
            "clear" => Command::Clear,
            "playnext" => Command::PlayNext,
            "queue" => Command::Queue,
            "play" => Command::Play,
            "update" => Command::UpdateLibrary,
            "delete" => Command::Delete,
            "back" => Command::Back,
            "open" => {
                let &target_mode_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("selected|current".into()),
                })?;
                let target_mode = match target_mode_raw {
                    "selected" => Ok(TargetMode::Selected),
                    "current" => Ok(TargetMode::Current),
                    _ => Err(BadEnumArg {
                        arg: target_mode_raw.into(),
                        accept: vec!["selected".into(), "current".into()],
                    }),
                }?;
                Command::Open(target_mode)
            }
            "jump" => Command::Jump(JumpMode::Query(args.join(" "))),
            "jumpnext" => Command::Jump(JumpMode::Next),
            "jumpprevious" => Command::Jump(JumpMode::Previous),
            "search" => Command::Search(args.join(" ")),
            "shift" => {
                let &shift_dir_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("up|down".into()),
                })?;
                let shift_dir = match shift_dir_raw {
                    "up" => Ok(ShiftMode::Up),
                    "down" => Ok(ShiftMode::Down),
                    _ => Err(BadEnumArg {
                        arg: shift_dir_raw.into(),
                        accept: vec!["up".into(), "down".into()],
                    }),
                }?;
                let amount = match args.get(1) {
                    Some(&amount_raw) => {
                        let amount = amount_raw.parse::<i32>().map_err(|err| ArgParseError {
                            arg: amount_raw.into(),
                            err: err.to_string(),
                        })?;
                        Some(amount)
                    }
                    None => None,
                };
                Command::Shift(shift_dir, amount)
            }
            "move" => {
                let &move_mode_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("a direction".into()),
                })?;
                let move_mode = {
                    use MoveMode::*;
                    match move_mode_raw {
                        "playing" => Ok(Playing),
                        "top" | "up" => Ok(Up),
                        "bottom" | "down" => Ok(Down),
                        "leftmost" | "left" => Ok(Left),
                        "rightmost" | "right" => Ok(Right),
                        _ => Err(BadEnumArg {
                            arg: move_mode_raw.into(),
                            accept: vec![
                                "playing".into(),
                                "top".into(),
                                "bottom".into(),
                                "leftmost".into(),
                                "rightmost".into(),
                                "up".into(),
                                "down".into(),
                                "left".into(),
                                "right".into(),
                            ],
                        }),
                    }?
                };
                let move_amount = match move_mode_raw {
                    "playing" => Ok(MoveAmount::default()),
                    "top" | "bottom" | "leftmost" | "rightmost" => Ok(MoveAmount::Extreme),
                    "up" | "down" | "left" | "right" => {
                        let amount = match args.get(1) {
                            Some(&amount_raw) => amount_raw
                                .parse::<i32>()
                                .map(MoveAmount::Integer)
                                .map_err(|err| ArgParseError {
                                    arg: amount_raw.into(),
                                    err: err.to_string(),
                                })?,
                            None => MoveAmount::default(),
                        };
                        Ok(amount)
                    }
                    _ => unreachable!(), // already guarded when determining MoveMode
                }?;
                Command::Move(move_mode, move_amount)
            }
            "goto" => {
                let &goto_mode_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("album|artist".into()),
                })?;
                let goto_mode = match goto_mode_raw {
                    "album" => Ok(GotoMode::Album),
                    "artist" => Ok(GotoMode::Artist),
                    _ => Err(BadEnumArg {
                        arg: goto_mode_raw.into(),
                        accept: vec!["album".into(), "artist".into()],
                    }),
                }?;
                Command::Goto(goto_mode)
            }
            "share" => {
                let &target_mode_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("selected|current".into()),
                })?;
                let target_mode = match target_mode_raw {
                    "selected" => Ok(TargetMode::Selected),
                    "current" => Ok(TargetMode::Current),
                    _ => Err(BadEnumArg {
                        arg: target_mode_raw.into(),
                        accept: vec!["selected".into(), "current".into()],
                    }),
                }?;
                Command::Share(target_mode)
            }
            "shuffle" => {
                let switch = match args.get(0).cloned() {
                    Some("on") => Ok(Some(true)),
                    Some("off") => Ok(Some(false)),
                    Some(arg) => Err(BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["**omit**".into(), "on".into(), "off".into()],
                    }),
                    None => Ok(None),
                }?;
                Command::Shuffle(switch)
            }
            "repeat" => {
                let mode = match args.get(0).cloned() {
                    Some("list" | "playlist" | "queue") => Ok(Some(RepeatSetting::RepeatPlaylist)),
                    Some("track" | "once" | "single") => Ok(Some(RepeatSetting::RepeatTrack)),
                    Some("none" | "off") => Ok(Some(RepeatSetting::None)),
                    Some(arg) => Err(BadEnumArg {
                        arg: arg.into(),
                        accept: vec![
                            "**omit**".into(),
                            "list".into(),
                            "playlist".into(),
                            "queue".into(),
                            "track".into(),
                            "once".into(),
                            "single".into(),
                            "none".into(),
                            "off".into(),
                        ],
                    }),
                    None => Ok(None),
                }?;
                Command::Repeat(mode)
            }
            "seek" => {
                let arg = args.join(" ");
                let first_char = arg.chars().next();
                let duration_raw = match first_char {
                    Some('+' | '-') => {
                        arg.chars().skip(1).collect::<String>().trim().into()
                        // `trim` is necessary here, otherwise `+1000` -> 1 second, but `+ 1000` -> 1000 seconds
                        // this behaviour is inconsistent and could cause confusion
                    }
                    _ => arg,
                };
                let unsigned_millis = match duration_raw.parse() {
                    // accept raw milliseconds
                    Ok(millis) => millis,
                    Err(_) => parse_duration::parse(&duration_raw) // accept fancy duration
                        .map_err(|err| ArgParseError {
                            arg: duration_raw.clone(),
                            err: err.to_string(),
                        })
                        .and_then(|dur| {
                            dur.as_millis().try_into().map_err(|_| ArgParseError {
                                arg: duration_raw.clone(),
                                err: "Duration value too large".into(),
                            })
                        })?,
                };
                let seek_direction = match first_char {
                    // handle i32::MAX < unsigned_millis < u32::MAX gracefully
                    Some('+') => {
                        i32::try_from(unsigned_millis).map(|millis| SeekDirection::Relative(millis))
                    }
                    Some('-') => i32::try_from(unsigned_millis)
                        .map(|millis| SeekDirection::Relative(-millis)),
                    _ => Ok(SeekDirection::Absolute(unsigned_millis)),
                }
                .map_err(|_| ArgParseError {
                    arg: duration_raw,
                    err: "Duration value too large".into(),
                })?;
                Command::Seek(seek_direction)
            }
            "focus" => {
                let &target = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("queue|search|library".into()),
                })?;
                // TODO: this really should be strongly typed
                Command::Focus(target.into())
            }
            "save" => match args.get(0).cloned() {
                Some("queue") => Ok(Command::SaveQueue),
                Some(arg) => Err(BadEnumArg {
                    arg: arg.into(),
                    accept: vec!["**omit**".into(), "queue".into()],
                }),
                None => Ok(Command::Save),
            }?,
            "volup" => {
                let amount = match args.get(0) {
                    Some(&amount_raw) => {
                        amount_raw.parse::<u16>().map_err(|err| ArgParseError {
                            arg: amount_raw.into(),
                            err: err.to_string(),
                        })?
                    }
                    None => 1,
                };
                Command::VolumeUp(amount)
            }
            "voldown" => {
                let amount = match args.get(0) {
                    Some(&amount_raw) => {
                        amount_raw.parse::<u16>().map_err(|err| ArgParseError {
                            arg: amount_raw.into(),
                            err: err.to_string(),
                        })?
                    }
                    None => 1,
                };
                Command::VolumeDown(amount)
            }
            "help" => Command::Help,
            "reload" => Command::ReloadConfig,
            "insert" => {
                // IDEA: this should fail fast too
                Command::Insert(args.get(0).map(|&url| url.into()))
            }
            "newplaylist" => {
                if !args.is_empty() {
                    Ok(Command::NewPlaylist(args.join(" ")))
                } else {
                    Err(InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("a name".into()),
                    })
                }?
            }
            "sort" => {
                let &key_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("a sort key".into()),
                })?;
                let key = match key_raw {
                    "title" => Ok(SortKey::Title),
                    "duration" => Ok(SortKey::Duration),
                    "album" => Ok(SortKey::Album),
                    "added" => Ok(SortKey::Added),
                    "artist" => Ok(SortKey::Artist),
                    _ => Err(BadEnumArg {
                        arg: key_raw.into(),
                        accept: vec![
                            "title".into(),
                            "duration".into(),
                            "album".into(),
                            "added".into(),
                            "artist".into(),
                        ],
                    }),
                }?;
                let direction = match args.get(1) {
                    Some(&direction_raw) => match direction_raw {
                        "a" | "asc" | "ascending" => Ok(SortDirection::Ascending),
                        "d" | "desc" | "descending" => Ok(SortDirection::Descending),
                        _ => Err(BadEnumArg {
                            arg: direction_raw.into(),
                            accept: vec![
                                "a".into(),
                                "asc".into(),
                                "ascending".into(),
                                "d".into(),
                                "desc".into(),
                                "descending".into(),
                            ],
                        }),
                    },
                    None => Ok(SortDirection::Ascending),
                }?;
                Command::Sort(key, direction)
            }
            "logout" => Command::Logout,
            "similar" => {
                let &target_mode_raw = args.get(0).ok_or(InsufficientArgs {
                    cmd: command.into(),
                    hint: Some("selected|current".into()),
                })?;
                let target_mode = match target_mode_raw {
                    "selected" => Ok(TargetMode::Selected),
                    "current" => Ok(TargetMode::Current),
                    _ => Err(BadEnumArg {
                        arg: target_mode_raw.into(),
                        accept: vec!["selected".into(), "current".into()],
                    }),
                }?;
                Command::ShowRecommendations(target_mode)
            }
            "noop" => Command::Noop,
            "redraw" => Command::Redraw,
            "exec" => Command::Execute(args.join(" ")),
            _ => Err(NoSuchCommand {
                cmd: command.into(),
            })?, // I'm surprised this compiles lol
        };
        commands.push(command);
    }
    Ok(commands)
}
