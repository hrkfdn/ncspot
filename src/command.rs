use crate::queue::RepeatSetting;
use crate::spotify_url::SpotifyUrl;
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
    Float(f32),
    Extreme,
}

impl Default for MoveAmount {
    fn default() -> Self {
        MoveAmount::Integer(1)
    }
}

/// Keys that can be used to sort songs on.
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
            SeekDirection::Absolute(pos) => format!("{pos}"),
            SeekDirection::Relative(delta) => {
                format!("{}{}", if delta > &0 { "+" } else { "" }, delta)
            }
        };
        write!(f, "{repr}")
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum InsertSource {
    #[cfg(feature = "share_clipboard")]
    Clipboard,
    Input(SpotifyUrl),
}

impl fmt::Display for InsertSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            #[cfg(feature = "share_clipboard")]
            InsertSource::Clipboard => "".into(),
            InsertSource::Input(url) => url.to_string(),
        };
        write!(f, "{repr}")
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
    SaveCurrent,
    SaveQueue,
    Add,
    AddCurrent,
    Delete,
    Focus(String),
    Seek(SeekDirection),
    VolumeUp(u16),
    VolumeDown(u16),
    Repeat(Option<RepeatSetting>),
    Shuffle(Option<bool>),
    #[cfg(feature = "share_clipboard")]
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
    Insert(InsertSource),
    NewPlaylist(String),
    Sort(SortKey, SortDirection),
    Logout,
    ShowRecommendations(TargetMode),
    Redraw,
    Execute(String),
    Reconnect,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut repr_tokens = vec![self.basename().to_owned()];
        let mut extras_args = match self {
            Command::Focus(tab) => vec![tab.to_owned()],
            Command::Seek(direction) => vec![direction.to_string()],
            Command::VolumeUp(amount) => vec![amount.to_string()],
            Command::VolumeDown(amount) => vec![amount.to_string()],
            Command::Repeat(mode) => match mode {
                Some(mode) => vec![mode.to_string()],
                None => vec![],
            },
            Command::Shuffle(on) => match on {
                Some(b) => vec![(if *b { "on" } else { "off" }).into()],
                None => vec![],
            },
            #[cfg(feature = "share_clipboard")]
            Command::Share(mode) => vec![mode.to_string()],
            Command::Open(mode) => vec![mode.to_string()],
            Command::Goto(mode) => vec![mode.to_string()],
            Command::Move(mode, amount) => match (mode, amount) {
                (MoveMode::Playing, _) => vec!["playing".to_string()],
                (MoveMode::Up, MoveAmount::Extreme) => vec!["top".to_string()],
                (MoveMode::Down, MoveAmount::Extreme) => vec!["bottom".to_string()],
                (MoveMode::Left, MoveAmount::Extreme) => vec!["leftmost".to_string()],
                (MoveMode::Right, MoveAmount::Extreme) => vec!["rightmost".to_string()],
                (mode, MoveAmount::Float(amount)) => vec![mode.to_string(), amount.to_string()],
                (mode, MoveAmount::Integer(amount)) => vec![mode.to_string(), amount.to_string()],
            },
            Command::Shift(mode, amount) => vec![mode.to_string(), amount.unwrap_or(1).to_string()],
            Command::Search(term) => vec![term.to_owned()],
            Command::Jump(mode) => match mode {
                JumpMode::Previous | JumpMode::Next => vec![],
                JumpMode::Query(term) => vec![term.to_owned()],
            },
            Command::Insert(source) => vec![source.to_string()],
            Command::NewPlaylist(name) => vec![name.to_owned()],
            Command::Sort(key, direction) => vec![key.to_string(), direction.to_string()],
            Command::ShowRecommendations(mode) => vec![mode.to_string()],
            Command::Execute(cmd) => vec![cmd.to_owned()],
            Command::Quit
            | Command::TogglePlay
            | Command::Stop
            | Command::Previous
            | Command::Next
            | Command::Clear
            | Command::Queue
            | Command::PlayNext
            | Command::Play
            | Command::UpdateLibrary
            | Command::Save
            | Command::SaveCurrent
            | Command::SaveQueue
            | Command::Add
            | Command::AddCurrent
            | Command::Delete
            | Command::Back
            | Command::Help
            | Command::ReloadConfig
            | Command::Noop
            | Command::Logout
            | Command::Reconnect
            | Command::Redraw => vec![],
        };
        repr_tokens.append(&mut extras_args);
        write!(f, "{}", repr_tokens.join(" "))
    }
}

impl Command {
    pub fn basename(&self) -> &str {
        match self {
            Command::Quit => "quit",
            Command::TogglePlay => "playpause",
            Command::Stop => "stop",
            Command::Previous => "previous",
            Command::Next => "next",
            Command::Clear => "clear",
            Command::Queue => "queue",
            Command::PlayNext => "playnext",
            Command::Play => "play",
            Command::UpdateLibrary => "update",
            Command::Save => "save",
            Command::SaveCurrent => "save current",
            Command::SaveQueue => "save queue",
            Command::Add => "add",
            Command::AddCurrent => "add current",
            Command::Delete => "delete",
            Command::Focus(_) => "focus",
            Command::Seek(_) => "seek",
            Command::VolumeUp(_) => "volup",
            Command::VolumeDown(_) => "voldown",
            Command::Repeat(_) => "repeat",
            Command::Shuffle(_) => "shuffle",
            #[cfg(feature = "share_clipboard")]
            Command::Share(_) => "share",
            Command::Back => "back",
            Command::Open(_) => "open",
            Command::Goto(_) => "goto",
            Command::Move(_, _) => "move",
            Command::Shift(_, _) => "shift",
            Command::Search(_) => "search",
            Command::Jump(JumpMode::Previous) => "jumpprevious",
            Command::Jump(JumpMode::Next) => "jumpnext",
            Command::Jump(JumpMode::Query(_)) => "jump",
            Command::Help => "help",
            Command::ReloadConfig => "reload",
            Command::Noop => "noop",
            Command::Insert(_) => "insert",
            Command::NewPlaylist(_) => "newplaylist",
            Command::Sort(_, _) => "sort",
            Command::Logout => "logout",
            Command::ShowRecommendations(_) => "similar",
            Command::Redraw => "redraw",
            Command::Execute(_) => "exec",
            Command::Reconnect => "reconnect",
        }
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
            NoSuchCommand { cmd } => format!("No such command \"{cmd}\""),
            InsufficientArgs { cmd, hint } => {
                if let Some(hint_str) = hint {
                    format!("\"{cmd}\" requires additional arguments: {hint_str}")
                } else {
                    format!("\"{cmd}\" requires additional arguments")
                }
            }
            BadEnumArg { arg, accept } => {
                format!(
                    "Illegal argument \"{}\": supported values are {}",
                    arg,
                    accept.join("|")
                )
            }
            ArgParseError { arg, err } => format!("Error with argument \"{arg}\": {err}"),
        };
        write!(f, "{formatted}")
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
        let components: Vec<_> = command_input.split_whitespace().collect();

        if let Some((command, args)) = components.split_first() {
            let command = handle_aliases(command);
            use CommandParseError::*;
            let command = match command {
                "quit" => Command::Quit,
                "playpause" => Command::TogglePlay,
                "stop" => Command::Stop,
                "previous" => Command::Previous,
                "next" => Command::Next,
                "clear" => Command::Clear,
                "queue" => Command::Queue,
                "playnext" => Command::PlayNext,
                "play" => Command::Play,
                "update" => Command::UpdateLibrary,
                "add" => match args.first().cloned() {
                    Some("current") => Ok(Command::AddCurrent),
                    Some(arg) => Err(BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["**omit**".into(), "queue".into()],
                    }),
                    None => Ok(Command::Add),
                }?,
                "save" => match args.first().cloned() {
                    Some("queue") => Ok(Command::SaveQueue),
                    Some("current") => Ok(Command::SaveCurrent),
                    Some(arg) => Err(BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["**omit**".into(), "queue".into()],
                    }),
                    None => Ok(Command::Save),
                }?,
                "delete" => Command::Delete,
                "focus" => {
                    let &target = args.first().ok_or(InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("queue|search|library".into()),
                    })?;
                    // TODO: this really should be strongly typed
                    Command::Focus(target.into())
                }
                "seek" => {
                    if args.is_empty() {
                        return Err(InsufficientArgs {
                            cmd: command.into(),
                            hint: Some("a duration".into()),
                        });
                    }
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
                        Some('+') => i32::try_from(unsigned_millis).map(SeekDirection::Relative),
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
                "volup" => {
                    let amount = match args.first() {
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
                    let amount = match args.first() {
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
                "repeat" => {
                    let mode = match args.first().cloned() {
                        Some("list" | "playlist" | "queue") => {
                            Ok(Some(RepeatSetting::RepeatPlaylist))
                        }
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
                "shuffle" => {
                    let switch = match args.first().cloned() {
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
                #[cfg(feature = "share_clipboard")]
                "share" => {
                    let &target_mode_raw = args.first().ok_or(InsufficientArgs {
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
                "back" => Command::Back,
                "open" => {
                    let &target_mode_raw = args.first().ok_or(InsufficientArgs {
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
                "goto" => {
                    let &goto_mode_raw = args.first().ok_or(InsufficientArgs {
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
                "move" => {
                    let &move_mode_raw = args.first().ok_or(InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("a direction".into()),
                    })?;
                    let move_mode = {
                        use MoveMode::*;
                        match move_mode_raw {
                            "playing" => Ok(Playing),
                            "top" | "pageup" | "up" => Ok(Up),
                            "bottom" | "pagedown" | "down" => Ok(Down),
                            "leftmost" | "pageleft" | "left" => Ok(Left),
                            "rightmost" | "pageright" | "right" => Ok(Right),
                            _ => Err(BadEnumArg {
                                arg: move_mode_raw.into(),
                                accept: vec![
                                    "playing".into(),
                                    "top".into(),
                                    "bottom".into(),
                                    "leftmost".into(),
                                    "rightmost".into(),
                                    "pageup".into(),
                                    "pagedown".into(),
                                    "pageleft".into(),
                                    "pageright".into(),
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
                        "pageup" | "pagedown" | "pageleft" | "pageright" => {
                            let amount = match args.get(1) {
                                Some(&amount_raw) => amount_raw
                                    .parse::<f32>()
                                    .map(MoveAmount::Float)
                                    .map_err(|err| ArgParseError {
                                        arg: amount_raw.into(),
                                        err: err.to_string(),
                                    })?,
                                None => MoveAmount::default(),
                            };
                            Ok(amount)
                        }
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
                "shift" => {
                    let &shift_dir_raw = args.first().ok_or(InsufficientArgs {
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
                            let amount =
                                amount_raw.parse::<i32>().map_err(|err| ArgParseError {
                                    arg: amount_raw.into(),
                                    err: err.to_string(),
                                })?;
                            Some(amount)
                        }
                        None => None,
                    };
                    Command::Shift(shift_dir, amount)
                }
                "search" => Command::Search(args.join(" ")),
                "jump" => Command::Jump(JumpMode::Query(args.join(" "))),
                "jumpnext" => Command::Jump(JumpMode::Next),
                "jumpprevious" => Command::Jump(JumpMode::Previous),
                "help" => Command::Help,
                "reload" => Command::ReloadConfig,
                "noop" => Command::Noop,
                "insert" => {
                    let insert_source = match args.first().cloned() {
                        #[cfg(feature = "share_clipboard")]
                        Some("") | None => Ok(InsertSource::Clipboard),
                        // if clipboard feature is disabled and args is empty
                        #[cfg(not(feature = "share_clipboard"))]
                        None => Err(InsufficientArgs {
                            cmd: command.into(),
                            hint: Some("a Spotify URL".into()),
                        }),
                        Some(url) => SpotifyUrl::from_url(url).map(InsertSource::Input).ok_or(
                            ArgParseError {
                                arg: url.into(),
                                err: "Invalid Spotify URL".into(),
                            },
                        ),
                    }?;
                    Command::Insert(insert_source)
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
                    let &key_raw = args.first().ok_or(InsufficientArgs {
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
                    let &target_mode_raw = args.first().ok_or(InsufficientArgs {
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
                "redraw" => Command::Redraw,
                "exec" => Command::Execute(args.join(" ")),
                "reconnect" => Command::Reconnect,
                _ => {
                    return Err(NoSuchCommand {
                        cmd: command.into(),
                    })
                }
            };
            commands.push(command);
        };
    }
    Ok(commands)
}
