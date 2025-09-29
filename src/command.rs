use crate::queue::RepeatSetting;
use crate::spotify_url::SpotifyUrl;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use strum_macros::Display;

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
        Self::Integer(1)
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
            Self::Absolute(pos) => format!("{pos}"),
            Self::Relative(delta) => {
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
            Self::Clipboard => "".into(),
            Self::Input(url) => url.to_string(),
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
            Self::Focus(tab) => vec![tab.to_owned()],
            Self::Seek(direction) => vec![direction.to_string()],
            Self::VolumeUp(amount) => vec![amount.to_string()],
            Self::VolumeDown(amount) => vec![amount.to_string()],
            Self::Repeat(mode) => match mode {
                Some(mode) => vec![mode.to_string()],
                None => vec![],
            },
            Self::Shuffle(on) => match on {
                Some(b) => vec![(if *b { "on" } else { "off" }).into()],
                None => vec![],
            },
            #[cfg(feature = "share_clipboard")]
            Self::Share(mode) => vec![mode.to_string()],
            Self::Open(mode) => vec![mode.to_string()],
            Self::Goto(mode) => vec![mode.to_string()],
            Self::Move(mode, amount) => match (mode, amount) {
                (MoveMode::Playing, _) => vec!["playing".to_string()],
                (MoveMode::Up, MoveAmount::Extreme) => vec!["top".to_string()],
                (MoveMode::Down, MoveAmount::Extreme) => vec!["bottom".to_string()],
                (MoveMode::Left, MoveAmount::Extreme) => vec!["leftmost".to_string()],
                (MoveMode::Right, MoveAmount::Extreme) => vec!["rightmost".to_string()],
                (mode, MoveAmount::Float(amount)) => vec![mode.to_string(), amount.to_string()],
                (mode, MoveAmount::Integer(amount)) => vec![mode.to_string(), amount.to_string()],
            },
            Self::Shift(mode, amount) => vec![mode.to_string(), amount.unwrap_or(1).to_string()],
            Self::Search(term) => vec![term.to_owned()],
            Self::Jump(mode) => match mode {
                JumpMode::Previous | JumpMode::Next => vec![],
                JumpMode::Query(term) => vec![term.to_owned()],
            },
            Self::Insert(source) => vec![source.to_string()],
            Self::NewPlaylist(name) => vec![name.to_owned()],
            Self::Sort(key, direction) => vec![key.to_string(), direction.to_string()],
            Self::ShowRecommendations(mode) => vec![mode.to_string()],
            Self::Execute(cmd) => vec![cmd.to_owned()],
            Self::Quit
            | Self::TogglePlay
            | Self::Stop
            | Self::Previous
            | Self::Next
            | Self::Clear
            | Self::Queue
            | Self::PlayNext
            | Self::Play
            | Self::UpdateLibrary
            | Self::Save
            | Self::SaveCurrent
            | Self::SaveQueue
            | Self::Add
            | Self::AddCurrent
            | Self::Delete
            | Self::Back
            | Self::Help
            | Self::ReloadConfig
            | Self::Noop
            | Self::Logout
            | Self::Reconnect
            | Self::Redraw => vec![],
        };
        repr_tokens.append(&mut extras_args);
        write!(f, "{}", repr_tokens.join(" "))
    }
}

impl Command {
    pub fn basename(&self) -> &str {
        match self {
            Self::Quit => "quit",
            Self::TogglePlay => "playpause",
            Self::Stop => "stop",
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Clear => "clear",
            Self::Queue => "queue",
            Self::PlayNext => "playnext",
            Self::Play => "play",
            Self::UpdateLibrary => "update",
            Self::Save => "save",
            Self::SaveCurrent => "save current",
            Self::SaveQueue => "save queue",
            Self::Add => "add",
            Self::AddCurrent => "add current",
            Self::Delete => "delete",
            Self::Focus(_) => "focus",
            Self::Seek(_) => "seek",
            Self::VolumeUp(_) => "volup",
            Self::VolumeDown(_) => "voldown",
            Self::Repeat(_) => "repeat",
            Self::Shuffle(_) => "shuffle",
            #[cfg(feature = "share_clipboard")]
            Self::Share(_) => "share",
            Self::Back => "back",
            Self::Open(_) => "open",
            Self::Goto(_) => "goto",
            Self::Move(_, _) => "move",
            Self::Shift(_, _) => "shift",
            Self::Search(_) => "search",
            Self::Jump(JumpMode::Previous) => "jumpprevious",
            Self::Jump(JumpMode::Next) => "jumpnext",
            Self::Jump(JumpMode::Query(_)) => "jump",
            Self::Help => "help",
            Self::ReloadConfig => "reload",
            Self::Noop => "noop",
            Self::Insert(_) => "insert",
            Self::NewPlaylist(_) => "newplaylist",
            Self::Sort(_, _) => "sort",
            Self::Logout => "logout",
            Self::ShowRecommendations(_) => "similar",
            Self::Redraw => "redraw",
            Self::Execute(_) => "exec",
            Self::Reconnect => "reconnect",
        }
    }
}

fn register_aliases(map: &mut HashMap<&str, &str>, cmd: &'static str, names: Vec<&'static str>) {
    for a in names {
        map.insert(a, cmd);
    }
}

fn handle_aliases(input: &str) -> &str {
    // NOTE: There is probably a better way to write this than a static HashMap. The HashMap doesn't
    // improve performance as there's far too few keys, and the use of static doesn't seem good.
    static ALIASES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

    let aliases = ALIASES.get_or_init(|| {
        let mut m = HashMap::new();

        register_aliases(&mut m, "quit", vec!["q", "x"]);
        register_aliases(
            &mut m,
            "playpause",
            vec!["pause", "toggleplay", "toggleplayback"],
        );
        register_aliases(&mut m, "repeat", vec!["loop"]);
        m
    });

    if let Some(cmd) = aliases.get(input) {
        handle_aliases(cmd)
    } else {
        input
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommandParseError {
    NoSuchCommand {
        cmd: String,
    },
    InsufficientArgs {
        cmd: String,
        hint: Option<String>,
    },
    BadEnumArg {
        arg: String,
        accept: Vec<String>,
        optional: bool,
    },
    ArgParseError {
        arg: String,
        err: String,
    },
}

impl fmt::Display for CommandParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = match self {
            Self::NoSuchCommand { cmd } => format!("No such command \"{cmd}\""),
            Self::InsufficientArgs { cmd, hint } => {
                if let Some(hint_str) = hint {
                    format!("\"{cmd}\" requires additional arguments: {hint_str}")
                } else {
                    format!("\"{cmd}\" requires additional arguments")
                }
            }
            Self::BadEnumArg {
                arg,
                accept,
                optional,
            } => {
                let accept = accept.join("|");
                if *optional {
                    format!("Argument \"{arg}\" should be one of {accept} or be omitted")
                } else {
                    format!("Argument \"{arg}\" should be one of {accept}")
                }
            }
            Self::ArgParseError { arg, err } => format!("Error with argument \"{arg}\": {err}"),
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
            use CommandParseError as E;
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
                    Some(arg) => Err(E::BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["current".into()],
                        optional: true,
                    }),
                    None => Ok(Command::Add),
                }?,
                "save" => match args.first().cloned() {
                    Some("queue") => Ok(Command::SaveQueue),
                    Some("current") => Ok(Command::SaveCurrent),
                    Some(arg) => Err(E::BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["queue".into(), "current".into()],
                        optional: true,
                    }),
                    None => Ok(Command::Save),
                }?,
                "delete" => Command::Delete,
                "focus" => {
                    let &target = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("queue|search|library".into()),
                    })?;
                    // TODO: this really should be strongly typed
                    Command::Focus(target.into())
                }
                "seek" => {
                    if args.is_empty() {
                        return Err(E::InsufficientArgs {
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
                            .map_err(|err| E::ArgParseError {
                                arg: duration_raw.clone(),
                                err: err.to_string(),
                            })
                            .and_then(|dur| {
                                dur.as_millis().try_into().map_err(|_| E::ArgParseError {
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
                    .map_err(|_| E::ArgParseError {
                        arg: duration_raw,
                        err: "Duration value too large".into(),
                    })?;
                    Command::Seek(seek_direction)
                }
                "volup" => {
                    let amount = match args.first() {
                        Some(&amount_raw) => {
                            amount_raw.parse::<u16>().map_err(|err| E::ArgParseError {
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
                            amount_raw.parse::<u16>().map_err(|err| E::ArgParseError {
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
                        Some(arg) => Err(E::BadEnumArg {
                            arg: arg.into(),
                            accept: vec![
                                "list".into(),
                                "playlist".into(),
                                "queue".into(),
                                "track".into(),
                                "once".into(),
                                "single".into(),
                                "none".into(),
                                "off".into(),
                            ],
                            optional: true,
                        }),
                        None => Ok(None),
                    }?;
                    Command::Repeat(mode)
                }
                "shuffle" => {
                    let switch = match args.first().cloned() {
                        Some("on") => Ok(Some(true)),
                        Some("off") => Ok(Some(false)),
                        Some(arg) => Err(E::BadEnumArg {
                            arg: arg.into(),
                            accept: vec!["on".into(), "off".into()],
                            optional: true,
                        }),
                        None => Ok(None),
                    }?;
                    Command::Shuffle(switch)
                }
                #[cfg(feature = "share_clipboard")]
                "share" => {
                    let &target_mode_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("selected|current".into()),
                    })?;
                    let target_mode = match target_mode_raw {
                        "selected" => Ok(TargetMode::Selected),
                        "current" => Ok(TargetMode::Current),
                        _ => Err(E::BadEnumArg {
                            arg: target_mode_raw.into(),
                            accept: vec!["selected".into(), "current".into()],
                            optional: false,
                        }),
                    }?;
                    Command::Share(target_mode)
                }
                "back" => Command::Back,
                "open" => {
                    let &target_mode_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("selected|current".into()),
                    })?;
                    let target_mode = match target_mode_raw {
                        "selected" => Ok(TargetMode::Selected),
                        "current" => Ok(TargetMode::Current),
                        _ => Err(E::BadEnumArg {
                            arg: target_mode_raw.into(),
                            accept: vec!["selected".into(), "current".into()],
                            optional: false,
                        }),
                    }?;
                    Command::Open(target_mode)
                }
                "goto" => {
                    let &goto_mode_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("album|artist".into()),
                    })?;
                    let goto_mode = match goto_mode_raw {
                        "album" => Ok(GotoMode::Album),
                        "artist" => Ok(GotoMode::Artist),
                        _ => Err(E::BadEnumArg {
                            arg: goto_mode_raw.into(),
                            accept: vec!["album".into(), "artist".into()],
                            optional: false,
                        }),
                    }?;
                    Command::Goto(goto_mode)
                }
                "move" => {
                    let &move_mode_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("a direction".into()),
                    })?;
                    let move_mode = {
                        use MoveMode as M;
                        match move_mode_raw {
                            "playing" => Ok(M::Playing),
                            "top" | "pageup" | "up" => Ok(M::Up),
                            "bottom" | "pagedown" | "down" => Ok(M::Down),
                            "leftmost" | "pageleft" | "left" => Ok(M::Left),
                            "rightmost" | "pageright" | "right" => Ok(M::Right),
                            _ => Err(E::BadEnumArg {
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
                                optional: false,
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
                                    .map_err(|err| E::ArgParseError {
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
                                    .map_err(|err| E::ArgParseError {
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
                    let &shift_dir_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("up|down".into()),
                    })?;
                    let shift_dir = match shift_dir_raw {
                        "up" => Ok(ShiftMode::Up),
                        "down" => Ok(ShiftMode::Down),
                        _ => Err(E::BadEnumArg {
                            arg: shift_dir_raw.into(),
                            accept: vec!["up".into(), "down".into()],
                            optional: false,
                        }),
                    }?;
                    let amount = match args.get(1) {
                        Some(&amount_raw) => {
                            let amount =
                                amount_raw.parse::<i32>().map_err(|err| E::ArgParseError {
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
                        None => Err(E::InsufficientArgs {
                            cmd: command.into(),
                            hint: Some("a Spotify URL".into()),
                        }),
                        Some(url) => SpotifyUrl::from_url(url).map(InsertSource::Input).ok_or(
                            E::ArgParseError {
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
                        Err(E::InsufficientArgs {
                            cmd: command.into(),
                            hint: Some("a name".into()),
                        })
                    }?
                }
                "sort" => {
                    let &key_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("a sort key".into()),
                    })?;
                    let key = match key_raw {
                        "title" => Ok(SortKey::Title),
                        "duration" => Ok(SortKey::Duration),
                        "album" => Ok(SortKey::Album),
                        "added" => Ok(SortKey::Added),
                        "artist" => Ok(SortKey::Artist),
                        _ => Err(E::BadEnumArg {
                            arg: key_raw.into(),
                            accept: vec![
                                "title".into(),
                                "duration".into(),
                                "album".into(),
                                "added".into(),
                                "artist".into(),
                            ],
                            optional: false,
                        }),
                    }?;
                    let direction = match args.get(1).copied() {
                        Some("a" | "asc" | "ascending") => Ok(SortDirection::Ascending),
                        Some("d" | "desc" | "descending") => Ok(SortDirection::Descending),
                        Some(direction_raw) => Err(E::BadEnumArg {
                            arg: direction_raw.into(),
                            accept: vec![
                                "a".into(),
                                "asc".into(),
                                "ascending".into(),
                                "d".into(),
                                "desc".into(),
                                "descending".into(),
                            ],
                            optional: true,
                        }),
                        None => Ok(SortDirection::Ascending),
                    }?;
                    Command::Sort(key, direction)
                }
                "logout" => Command::Logout,
                "similar" => {
                    let &target_mode_raw = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("selected|current".into()),
                    })?;
                    let target_mode = match target_mode_raw {
                        "selected" => Ok(TargetMode::Selected),
                        "current" => Ok(TargetMode::Current),
                        _ => Err(E::BadEnumArg {
                            arg: target_mode_raw.into(),
                            accept: vec!["selected".into(), "current".into()],
                            optional: false,
                        }),
                    }?;
                    Command::ShowRecommendations(target_mode)
                }
                "redraw" => Command::Redraw,
                "exec" => Command::Execute(args.join(" ")),
                "reconnect" => Command::Reconnect,
                _ => {
                    return Err(E::NoSuchCommand {
                        cmd: command.into(),
                    });
                }
            };
            commands.push(command);
        };
    }
    Ok(commands)
}
