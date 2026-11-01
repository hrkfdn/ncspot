use crate::queue::RepeatSetting;
use crate::spotify_url::SpotifyUrl;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use strum_macros::Display;

const QUIT_CMD: &str = "quit";
const TOGGLE_PLAY_CMD: &str = "playpause";
const STOP_CMD: &str = "stop";
const PREVIOUS_CMD: &str = "previous";
const NEXT_CMD: &str = "next";
const CLEAR_CMD: &str = "clear";
const QUEUE_CMD: &str = "queue";
const PLAY_NEXT_CMD: &str = "playnext";
const PLAY: &str = "play";
const UPDATE_LIBRARY_CMD: &str = "update";
const UPDATE_TAB_CMD: &str = "update tab";
const SAVE_CMD: &str = "save";
const SAVE_CURRENT_CMD: &str = "save current";
const SAVE_QUEUE_CMD: &str = "save queue";
const ADD: &str = "add";
const ADD_CURRENT_CMD: &str = "add current";
const DELETE_CMD: &str = "delete";
const PIN_CMD: &str = "pin";
const UNPIN_CMD: &str = "unpin";
const FOCUS: &str = "focus";
const SEEK: &str = "seek";
const VOLUME_UP_CMD: &str = "volup";
const VOLUME_DOWN_CMD: &str = "voldown";
const REPEAT_CMD: &str = "repeat";
const SHUFFLE_CMD: &str = "shuffle";
#[cfg(feature = "share_clipboard")]
const SHARE_CMD: &str = "share";
const BACK_CMD: &str = "back";
const OPEN_CMD: &str = "open";
const GOTO_CMD: &str = "goto";
const MOVE_CMD: &str = "move";
const SHIFT_CMD: &str = "shift";
const SEARCH_CMD: &str = "search";
const JUMP_PREVIOUS_CMD: &str = "jumpprevious";
const JUMP_NEXT_CMD: &str = "jumpnext";
const JUMP_CMD: &str = "jump";
const HELP_CMD: &str = "help";
const RELOAD_CONFIG_CMD: &str = "reload";
const NOOP_CMD: &str = "noop";
const INSERT_CMD: &str = "insert";
const NEW_PLAYLIST_CMD: &str = "newplaylist";
const RENAME_CMD: &str = "rename";
const SORT_CMD: &str = "sort";
const LOGOUT_CMD: &str = "logout";
const SHOW_RECOMMENDATIONS_CMD: &str = "similar";
const REDRAW_CMD: &str = "redraw";
const EXECUTE_CMD: &str = "exec";
const RECONNECT_CMD: &str = "reconnect";
const LYRICS_SCROLL_UP_CMD: &str = "lyricsscrollup";
const LYRICS_SCROLL_DOWN_CMD: &str = "lyricsscrolldown";
const LYRICS_OFFSET_CMD: &str = "lyricsoffset";
const LYRICS_PROVIDER_CMD: &str = "lyricsprovider";
const LYRICS_REFETCH_CMD: &str = "lyricsrefetch";
const LYRICS_COPY_LINE_CMD: &str = "lyricscopyline";
const LYRICS_COPY_ALL_CMD: &str = "lyricscopyall";
const LYRICS_SEEK_CMD: &str = "lyricsseek";
const JUKEBOX_TOGGLE_CMD: &str = "jukeboxtoggle";
const JUKEBOX_VIEW_CMD: &str = "jukeboxview";
const JUKEBOX_BOUNCE_CMD: &str = "jukeboxbounce";
const JUKEBOX_SEEK_CMD: &str = "jukeboxseek";
const JUKEBOX_SETTINGS_CMD: &str = "jukeboxsettings";
const JUKEBOX_GRAPHICS_CMD: &str = "jukeboxgraphics";
const JUKEBOX_SONG_SETTINGS_CMD: &str = "jukeboxsongsettings";
#[cfg(feature = "harness")]
const HARNESS_CMD: &str = "harness";

pub const ALL_COMMANDS: &[&str] = &[
    QUIT_CMD,
    TOGGLE_PLAY_CMD,
    STOP_CMD,
    PREVIOUS_CMD,
    NEXT_CMD,
    CLEAR_CMD,
    QUEUE_CMD,
    PLAY_NEXT_CMD,
    PLAY,
    UPDATE_LIBRARY_CMD,
    UPDATE_TAB_CMD,
    SAVE_CMD,
    SAVE_CURRENT_CMD,
    SAVE_QUEUE_CMD,
    ADD,
    ADD_CURRENT_CMD,
    DELETE_CMD,
    PIN_CMD,
    UNPIN_CMD,
    FOCUS,
    SEEK,
    VOLUME_UP_CMD,
    VOLUME_DOWN_CMD,
    REPEAT_CMD,
    SHUFFLE_CMD,
    #[cfg(feature = "share_clipboard")]
    SHARE_CMD,
    BACK_CMD,
    OPEN_CMD,
    GOTO_CMD,
    MOVE_CMD,
    SHIFT_CMD,
    SEARCH_CMD,
    JUMP_PREVIOUS_CMD,
    JUMP_NEXT_CMD,
    JUMP_CMD,
    HELP_CMD,
    RELOAD_CONFIG_CMD,
    NOOP_CMD,
    INSERT_CMD,
    NEW_PLAYLIST_CMD,
    RENAME_CMD,
    SORT_CMD,
    LOGOUT_CMD,
    SHOW_RECOMMENDATIONS_CMD,
    REDRAW_CMD,
    EXECUTE_CMD,
    RECONNECT_CMD,
    LYRICS_SCROLL_UP_CMD,
    LYRICS_SCROLL_DOWN_CMD,
    LYRICS_OFFSET_CMD,
    LYRICS_PROVIDER_CMD,
    LYRICS_REFETCH_CMD,
    LYRICS_COPY_LINE_CMD,
    LYRICS_COPY_ALL_CMD,
    LYRICS_SEEK_CMD,
    JUKEBOX_TOGGLE_CMD,
    JUKEBOX_VIEW_CMD,
    JUKEBOX_BOUNCE_CMD,
    JUKEBOX_SEEK_CMD,
    JUKEBOX_SETTINGS_CMD,
    JUKEBOX_GRAPHICS_CMD,
    JUKEBOX_SONG_SETTINGS_CMD,
];

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
    /// Server order: no saved sort.
    Custom,
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

/// Drive harness subcommands (feature `harness`).
#[cfg(feature = "harness")]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum HarnessCommand {
    /// Push the view for a Spotify URI or URL; validated when run.
    Open(String),
    /// Show the library screen, optionally on one tab (a `LibraryTab` config name).
    OpenLibrary(Option<String>),
    /// Save a Spotify URI to the library (refuses one already saved).
    Save(String),
    /// Remove a Spotify URI from the library (refuses one not saved).
    Unsave(String),
    /// Create the next `ncspot-test-<k>` playlist and record its URI in the test-playlists file.
    NewPlaylistTest,
    /// Put the recorded test playlists' names in the next dump.
    TestPlaylists,
    /// Unfollow the n-th (1-based) recorded test playlist, then drop it from the file.
    UnfollowRecorded(usize),
    /// Append the first n public test tracks to the most recent test playlist.
    PlaylistAddTest(usize),
    /// Remove the track at an index from the most recent test playlist.
    PlaylistRemoveTest(usize),
    /// Replace the most recent test playlist's contents with the first n public test tracks.
    PlaylistOverwriteTest(usize),
    /// Shut the playback session down so the worker has to reconnect.
    DropSession,
    /// Write a JSON snapshot of the current view to a file.
    Dump(String),
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
    UpdateTab,
    Save,
    SaveCurrent,
    SaveQueue,
    Add,
    AddCurrent,
    Delete,
    Pin,
    Unpin,
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
    Rename(String),
    Sort(SortKey, SortDirection),
    Logout,
    ShowRecommendations(TargetMode),
    Redraw,
    Execute(String),
    Reconnect,
    LyricsScrollUp,
    LyricsScrollDown,
    LyricsOffset(i64),
    LyricsProviderCycle,
    LyricsRefetch,
    LyricsCopyLine,
    LyricsCopyAll,
    LyricsSeek,
    JukeboxToggle,
    JukeboxViewCycle,
    JukeboxBounce,
    JukeboxSeekToBeat,
    JukeboxSettings,
    JukeboxGraphics,
    JukeboxSongSettings,
    #[cfg(feature = "harness")]
    Harness(HarnessCommand),
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
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::Open(target)) => vec!["open".into(), target.clone()],
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::Dump(path)) => vec!["dump".into(), path.clone()],
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::OpenLibrary(tab)) => {
                let mut args = vec!["open".to_string(), "library".to_string()];
                args.extend(tab.clone());
                args
            }
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::Save(uri)) => vec!["save".into(), uri.clone()],
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::Unsave(uri)) => vec!["unsave".into(), uri.clone()],
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::NewPlaylistTest) => vec!["newplaylist-test".into()],
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::TestPlaylists) => vec!["test-playlists".into()],
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::UnfollowRecorded(n)) => {
                vec!["unfollow-recorded".into(), n.to_string()]
            }
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::PlaylistAddTest(n)) => {
                vec!["playlist-add-test".into(), n.to_string()]
            }
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::PlaylistRemoveTest(n)) => {
                vec!["playlist-remove-test".into(), n.to_string()]
            }
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::PlaylistOverwriteTest(n)) => {
                vec!["playlist-overwrite-test".into(), n.to_string()]
            }
            #[cfg(feature = "harness")]
            Self::Harness(HarnessCommand::DropSession) => vec!["drop-session".into()],
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
            Self::Rename(name) => vec![name.to_owned()],
            // Custom has one direction, so it has no direction argument.
            Self::Sort(SortKey::Custom, _) => vec![SortKey::Custom.to_string()],
            Self::Sort(key, direction) => vec![key.to_string(), direction.to_string()],
            Self::ShowRecommendations(mode) => vec![mode.to_string()],
            Self::Execute(cmd) => vec![cmd.to_owned()],
            Self::LyricsOffset(delta) => vec![delta.to_string()],
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
            | Self::UpdateTab
            | Self::Save
            | Self::SaveCurrent
            | Self::SaveQueue
            | Self::Add
            | Self::AddCurrent
            | Self::Delete
            | Self::Pin
            | Self::Unpin
            | Self::Back
            | Self::Help
            | Self::ReloadConfig
            | Self::Noop
            | Self::Logout
            | Self::Reconnect
            | Self::Redraw
            | Self::LyricsScrollUp
            | Self::LyricsScrollDown
            | Self::LyricsProviderCycle
            | Self::LyricsRefetch
            | Self::LyricsCopyLine
            | Self::LyricsCopyAll
            | Self::LyricsSeek
            | Self::JukeboxToggle
            | Self::JukeboxViewCycle
            | Self::JukeboxBounce
            | Self::JukeboxSeekToBeat
            | Self::JukeboxSettings
            | Self::JukeboxGraphics
            | Self::JukeboxSongSettings => vec![],
        };
        repr_tokens.append(&mut extras_args);
        write!(f, "{}", repr_tokens.join(" "))
    }
}

impl Command {
    pub fn basename(&self) -> &str {
        match self {
            Self::Quit => QUIT_CMD,
            Self::TogglePlay => TOGGLE_PLAY_CMD,
            Self::Stop => STOP_CMD,
            Self::Previous => PREVIOUS_CMD,
            Self::Next => NEXT_CMD,
            Self::Clear => CLEAR_CMD,
            Self::Queue => QUEUE_CMD,
            Self::PlayNext => PLAY_NEXT_CMD,
            Self::Play => PLAY,
            Self::UpdateLibrary => UPDATE_LIBRARY_CMD,
            Self::UpdateTab => UPDATE_TAB_CMD,
            Self::Save => SAVE_CMD,
            Self::SaveCurrent => SAVE_CURRENT_CMD,
            Self::SaveQueue => SAVE_QUEUE_CMD,
            Self::Add => ADD,
            Self::AddCurrent => ADD_CURRENT_CMD,
            Self::Delete => DELETE_CMD,
            Self::Pin => PIN_CMD,
            Self::Unpin => UNPIN_CMD,
            Self::Focus(_) => FOCUS,
            Self::Seek(_) => SEEK,
            Self::VolumeUp(_) => VOLUME_UP_CMD,
            Self::VolumeDown(_) => VOLUME_DOWN_CMD,
            Self::Repeat(_) => REPEAT_CMD,
            Self::Shuffle(_) => SHUFFLE_CMD,
            #[cfg(feature = "share_clipboard")]
            Self::Share(_) => SHARE_CMD,
            Self::Back => BACK_CMD,
            Self::Open(_) => OPEN_CMD,
            Self::Goto(_) => GOTO_CMD,
            Self::Move(_, _) => MOVE_CMD,
            Self::Shift(_, _) => SHIFT_CMD,
            Self::Search(_) => SEARCH_CMD,
            Self::Jump(JumpMode::Previous) => JUMP_PREVIOUS_CMD,
            Self::Jump(JumpMode::Next) => JUMP_NEXT_CMD,
            Self::Jump(JumpMode::Query(_)) => JUMP_CMD,
            Self::Help => HELP_CMD,
            Self::ReloadConfig => RELOAD_CONFIG_CMD,
            Self::Noop => NOOP_CMD,
            Self::Insert(_) => INSERT_CMD,
            Self::NewPlaylist(_) => NEW_PLAYLIST_CMD,
            Self::Rename(_) => RENAME_CMD,
            Self::Sort(_, _) => SORT_CMD,
            Self::Logout => LOGOUT_CMD,
            Self::ShowRecommendations(_) => SHOW_RECOMMENDATIONS_CMD,
            Self::Redraw => REDRAW_CMD,
            Self::Execute(_) => EXECUTE_CMD,
            Self::Reconnect => RECONNECT_CMD,
            Self::LyricsScrollUp => LYRICS_SCROLL_UP_CMD,
            Self::LyricsScrollDown => LYRICS_SCROLL_DOWN_CMD,
            Self::LyricsOffset(_) => LYRICS_OFFSET_CMD,
            Self::LyricsProviderCycle => LYRICS_PROVIDER_CMD,
            Self::LyricsRefetch => LYRICS_REFETCH_CMD,
            Self::LyricsCopyLine => LYRICS_COPY_LINE_CMD,
            Self::LyricsCopyAll => LYRICS_COPY_ALL_CMD,
            Self::LyricsSeek => LYRICS_SEEK_CMD,
            Self::JukeboxToggle => JUKEBOX_TOGGLE_CMD,
            Self::JukeboxViewCycle => JUKEBOX_VIEW_CMD,
            Self::JukeboxBounce => JUKEBOX_BOUNCE_CMD,
            Self::JukeboxSeekToBeat => JUKEBOX_SEEK_CMD,
            Self::JukeboxSettings => JUKEBOX_SETTINGS_CMD,
            Self::JukeboxGraphics => JUKEBOX_GRAPHICS_CMD,
            Self::JukeboxSongSettings => JUKEBOX_SONG_SETTINGS_CMD,
            #[cfg(feature = "harness")]
            Self::Harness(_) => HARNESS_CMD,
        }
    }

    /// Whether a vim-style count prefix (e.g. `10j`) may repeat this command.
    /// True for movement, seeking, volume, track skipping and search-jumps. A
    /// binding accepts a count only if all of its commands do.
    pub fn accepts_count(&self) -> bool {
        matches!(
            self,
            Self::Move(_, _)
                | Self::Queue
                | Self::PlayNext
                | Self::Next
                | Self::Previous
                | Self::Seek(_)
                | Self::VolumeUp(_)
                | Self::VolumeDown(_)
                | Self::Jump(_)
        )
    }

    /// Returns a single command equivalent to executing `self` `count` times.
    ///
    /// This optimization is used when repetition simply scales a single
    /// parameter. It allows a count-prefixed binding to act once rather than
    /// looping, which prevents redundant side effects (such as re-reading the
    /// async player position on every seek, or sending an individual volume
    /// update per step).
    ///
    /// Returns `None` if the command cannot be scaled and must be executed
    /// sequentially (e.g., movement, jumps, queueing, or track skips). In these
    /// cases, the caller is responsible for looping the execution.
    ///
    /// Expects `count` within `MAX_COUNT`; larger values truncate when cast to
    /// the parameter's type. The sole caller passes a `MAX_COUNT`-clamped count.
    pub fn scaled(&self, count: usize) -> Option<Self> {
        match self {
            Self::Seek(SeekDirection::Relative(delta)) => Some(Self::Seek(
                SeekDirection::Relative(delta.saturating_mul(count as i32)),
            )),
            Self::VolumeUp(amount) => Some(Self::VolumeUp(amount.saturating_mul(count as u16))),
            Self::VolumeDown(amount) => Some(Self::VolumeDown(amount.saturating_mul(count as u16))),
            _ => None,
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

        register_aliases(&mut m, QUIT_CMD, vec!["q", "x"]);
        register_aliases(
            &mut m,
            TOGGLE_PLAY_CMD,
            vec!["pause", "toggleplay", "toggleplayback"],
        );
        register_aliases(&mut m, REPEAT_CMD, vec!["loop"]);
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
                QUIT_CMD => Command::Quit,
                TOGGLE_PLAY_CMD => Command::TogglePlay,
                STOP_CMD => Command::Stop,
                PREVIOUS_CMD => Command::Previous,
                NEXT_CMD => Command::Next,
                CLEAR_CMD => Command::Clear,
                QUEUE_CMD => Command::Queue,
                PLAY_NEXT_CMD => Command::PlayNext,
                PLAY => Command::Play,
                UPDATE_LIBRARY_CMD => match args.first().cloned() {
                    Some("tab") => Ok(Command::UpdateTab),
                    Some(arg) => Err(E::BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["tab".into()],
                        optional: true,
                    }),
                    None => Ok(Command::UpdateLibrary),
                }?,
                ADD => match args.first().cloned() {
                    Some("current") => Ok(Command::AddCurrent),
                    Some(arg) => Err(E::BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["current".into()],
                        optional: true,
                    }),
                    None => Ok(Command::Add),
                }?,
                SAVE_CMD => match args.first().cloned() {
                    Some("queue") => Ok(Command::SaveQueue),
                    Some("current") => Ok(Command::SaveCurrent),
                    Some(arg) => Err(E::BadEnumArg {
                        arg: arg.into(),
                        accept: vec!["queue".into(), "current".into()],
                        optional: true,
                    }),
                    None => Ok(Command::Save),
                }?,
                DELETE_CMD => Command::Delete,
                PIN_CMD => Command::Pin,
                UNPIN_CMD => Command::Unpin,
                FOCUS => {
                    let &target = args.first().ok_or(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some("queue|search|library".into()),
                    })?;
                    // TODO: this really should be strongly typed
                    Command::Focus(target.into())
                }
                SEEK => {
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
                VOLUME_UP_CMD => {
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
                VOLUME_DOWN_CMD => {
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
                REPEAT_CMD => {
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
                SHUFFLE_CMD => {
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
                SHARE_CMD => {
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
                BACK_CMD => Command::Back,
                OPEN_CMD => {
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
                GOTO_CMD => {
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
                MOVE_CMD => {
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
                SHIFT_CMD => {
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
                SEARCH_CMD => Command::Search(args.join(" ")),
                JUMP_CMD => Command::Jump(JumpMode::Query(args.join(" "))),
                JUMP_NEXT_CMD => Command::Jump(JumpMode::Next),
                JUMP_PREVIOUS_CMD => Command::Jump(JumpMode::Previous),
                HELP_CMD => Command::Help,
                RELOAD_CONFIG_CMD => Command::ReloadConfig,
                NOOP_CMD => Command::Noop,
                INSERT_CMD => {
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
                NEW_PLAYLIST_CMD => {
                    if !args.is_empty() {
                        Ok(Command::NewPlaylist(args.join(" ")))
                    } else {
                        Err(E::InsufficientArgs {
                            cmd: command.into(),
                            hint: Some("a name".into()),
                        })
                    }?
                }
                RENAME_CMD => {
                    if !args.is_empty() {
                        Ok(Command::Rename(args.join(" ")))
                    } else {
                        Err(E::InsufficientArgs {
                            cmd: command.into(),
                            hint: Some("a name".into()),
                        })
                    }?
                }
                SORT_CMD => {
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
                        "custom" => Ok(SortKey::Custom),
                        _ => Err(E::BadEnumArg {
                            arg: key_raw.into(),
                            accept: vec![
                                "title".into(),
                                "duration".into(),
                                "album".into(),
                                "added".into(),
                                "artist".into(),
                                "custom".into(),
                            ],
                            optional: false,
                        }),
                    }?;
                    if let (SortKey::Custom, Some(&direction_raw)) = (&key, args.get(1)) {
                        return Err(E::ArgParseError {
                            arg: direction_raw.into(),
                            err: "custom order has one direction".into(),
                        });
                    }
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
                LOGOUT_CMD => Command::Logout,
                SHOW_RECOMMENDATIONS_CMD => {
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
                REDRAW_CMD => Command::Redraw,
                EXECUTE_CMD => Command::Execute(args.join(" ")),
                RECONNECT_CMD => Command::Reconnect,
                LYRICS_SCROLL_UP_CMD => Command::LyricsScrollUp,
                LYRICS_SCROLL_DOWN_CMD => Command::LyricsScrollDown,
                LYRICS_OFFSET_CMD => {
                    Command::LyricsOffset(args.first().and_then(|a| a.parse().ok()).unwrap_or(100))
                }
                LYRICS_PROVIDER_CMD => Command::LyricsProviderCycle,
                LYRICS_REFETCH_CMD => Command::LyricsRefetch,
                LYRICS_COPY_LINE_CMD => Command::LyricsCopyLine,
                LYRICS_COPY_ALL_CMD => Command::LyricsCopyAll,
                LYRICS_SEEK_CMD => Command::LyricsSeek,
                JUKEBOX_TOGGLE_CMD => Command::JukeboxToggle,
                JUKEBOX_VIEW_CMD => Command::JukeboxViewCycle,
                JUKEBOX_BOUNCE_CMD => Command::JukeboxBounce,
                JUKEBOX_SEEK_CMD => Command::JukeboxSeekToBeat,
                JUKEBOX_SETTINGS_CMD => Command::JukeboxSettings,
                JUKEBOX_GRAPHICS_CMD => Command::JukeboxGraphics,
                JUKEBOX_SONG_SETTINGS_CMD => Command::JukeboxSongSettings,
                #[cfg(feature = "harness")]
                HARNESS_CMD => match (args.first().copied(), args.get(1..).unwrap_or_default()) {
                    (Some("open"), ["library", rest @ ..]) => Ok(Command::Harness(
                        HarnessCommand::OpenLibrary(rest.first().map(|s| (*s).to_string())),
                    )),
                    (Some("drop-session"), []) => Ok(Command::Harness(HarnessCommand::DropSession)),
                    (Some("newplaylist-test"), []) => {
                        Ok(Command::Harness(HarnessCommand::NewPlaylistTest))
                    }
                    (Some("test-playlists"), []) => {
                        Ok(Command::Harness(HarnessCommand::TestPlaylists))
                    }
                    (Some("unfollow-recorded"), [n]) => n
                        .parse()
                        .map(|n| Command::Harness(HarnessCommand::UnfollowRecorded(n)))
                        .map_err(|_| E::InsufficientArgs {
                            cmd: format!("{command} unfollow-recorded"),
                            hint: Some("a recorded playlist number".into()),
                        }),
                    (Some(sub @ ("playlist-add-test" | "playlist-remove-test" | "playlist-overwrite-test")), [n]) => n
                        .parse()
                        .map(|n| {
                            Command::Harness(match sub {
                                "playlist-add-test" => HarnessCommand::PlaylistAddTest(n),
                                "playlist-remove-test" => HarnessCommand::PlaylistRemoveTest(n),
                                _ => HarnessCommand::PlaylistOverwriteTest(n),
                            })
                        })
                        .map_err(|_| E::InsufficientArgs {
                            cmd: format!("{command} {sub}"),
                            hint: Some("a number".into()),
                        }),
                    (Some("save"), [target]) => Ok(Command::Harness(HarnessCommand::Save(
                        (*target).to_string(),
                    ))),
                    (Some("unsave"), [target]) => Ok(Command::Harness(HarnessCommand::Unsave(
                        (*target).to_string(),
                    ))),
                    (Some("open"), [target, ..]) => Ok(Command::Harness(HarnessCommand::Open(
                        (*target).to_string(),
                    ))),
                    (Some("dump"), rest) if !rest.is_empty() => {
                        Ok(Command::Harness(HarnessCommand::Dump(rest.join(" "))))
                    }
                    (
                        Some(
                            sub @ ("open" | "dump" | "save" | "unsave" | "unfollow-recorded"
                            | "playlist-add-test" | "playlist-remove-test"
                            | "playlist-overwrite-test"),
                        ),
                        _,
                    ) => {
                        Err(E::InsufficientArgs {
                            cmd: format!("{command} {sub}"),
                            hint: Some(
                                match sub {
                                    "dump" => "a file path",
                                    "unfollow-recorded" => "a recorded playlist number",
                                    "playlist-add-test" | "playlist-remove-test"
                                    | "playlist-overwrite-test" => "a number",
                                    _ => "a Spotify URI or URL",
                                }
                                .into(),
                            ),
                        })
                    }
                    (Some(sub), _) => Err(E::BadEnumArg {
                        arg: sub.into(),
                        accept: vec![
                            "open".into(),
                            "dump".into(),
                            "drop-session".into(),
                            "save".into(),
                            "unsave".into(),
                            "newplaylist-test".into(),
                            "test-playlists".into(),
                            "unfollow-recorded".into(),
                            "playlist-add-test".into(),
                            "playlist-remove-test".into(),
                            "playlist-overwrite-test".into(),
                        ],
                        optional: false,
                    }),
                    (None, _) => Err(E::InsufficientArgs {
                        cmd: command.into(),
                        hint: Some(
                            "open|dump|drop-session|save|unsave|newplaylist-test|test-playlists|unfollow-recorded|playlist-add-test|playlist-remove-test|playlist-overwrite-test"
                                .into(),
                        ),
                    }),
                }?,
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

#[cfg(test)]
mod tests {
    #[test]
    fn parses_rename_with_the_rest_of_the_line() {
        assert!(matches!(
            parse("rename my new name").as_deref(),
            Ok([Command::Rename(name)]) if name == "my new name"
        ));
        assert!(parse("rename").is_err(), "a name is required");
        assert_eq!(Command::Rename("a b".into()).to_string(), "rename a b");
    }

    #[test]
    fn parses_sort_custom_without_a_direction() {
        assert!(matches!(
            parse("sort custom").as_deref(),
            Ok([Command::Sort(SortKey::Custom, _)])
        ));
        let err = parse("sort custom desc").unwrap_err().to_string();
        assert!(err.contains("custom order has one direction"), "{err}");
        assert_eq!(
            Command::Sort(SortKey::Custom, SortDirection::Ascending).to_string(),
            "sort custom"
        );
        assert_eq!(
            Command::Sort(SortKey::Title, SortDirection::Descending).to_string(),
            "sort title descending"
        );
    }

    use super::*;

    #[cfg(feature = "harness")]
    #[test]
    fn parses_harness_commands() {
        let open = parse("harness open spotify:album:abc").unwrap();
        assert!(matches!(
            open.as_slice(),
            [Command::Harness(HarnessCommand::Open(t))] if t == "spotify:album:abc"
        ));
        let dump = parse("harness dump /tmp/x/dump.json").unwrap();
        assert!(matches!(
            dump.as_slice(),
            [Command::Harness(HarnessCommand::Dump(p))] if p == "/tmp/x/dump.json"
        ));
        assert_eq!(dump[0].to_string(), "harness dump /tmp/x/dump.json");
        for (line, expect) in [
            ("harness open library", HarnessCommand::OpenLibrary(None)),
            (
                "harness open library episodes",
                HarnessCommand::OpenLibrary(Some("episodes".into())),
            ),
            ("harness drop-session", HarnessCommand::DropSession),
            ("harness newplaylist-test", HarnessCommand::NewPlaylistTest),
            ("harness test-playlists", HarnessCommand::TestPlaylists),
            (
                "harness unfollow-recorded 2",
                HarnessCommand::UnfollowRecorded(2),
            ),
            (
                "harness playlist-add-test 2",
                HarnessCommand::PlaylistAddTest(2),
            ),
            (
                "harness playlist-remove-test 0",
                HarnessCommand::PlaylistRemoveTest(0),
            ),
            (
                "harness playlist-overwrite-test 3",
                HarnessCommand::PlaylistOverwriteTest(3),
            ),
            (
                "harness save spotify:track:4uLU6hMCjMI75M1A2tKUQC",
                HarnessCommand::Save("spotify:track:4uLU6hMCjMI75M1A2tKUQC".into()),
            ),
            (
                "harness unsave spotify:show:4rOoJ6Egrf8K2IrywzwOMk",
                HarnessCommand::Unsave("spotify:show:4rOoJ6Egrf8K2IrywzwOMk".into()),
            ),
        ] {
            let cmds = parse(line).unwrap();
            assert!(matches!(cmds.as_slice(), [Command::Harness(c)] if *c == expect));
            assert_eq!(cmds[0].to_string(), line);
        }
        assert!(parse("harness").is_err());
        assert!(parse("harness open").is_err());
        assert!(parse("harness fly away").is_err());
        assert!(parse("harness unfollow-recorded").is_err());
        assert!(parse("harness unfollow-recorded x").is_err());
        assert!(parse("harness unfollow-recorded -1").is_err());
        assert!(parse("harness playlist-add-test").is_err());
        assert!(parse("harness playlist-remove-test x").is_err());
        assert!(parse("harness playlist-overwrite-test -1").is_err());
    }

    #[test]
    fn parses_pin_and_unpin() {
        let pin = parse("pin").unwrap();
        assert!(matches!(pin.as_slice(), [Command::Pin]));
        assert_eq!(pin[0].to_string(), "pin");
        assert!(matches!(
            parse(&pin[0].to_string()).as_deref(),
            Ok([Command::Pin])
        ));
        let unpin = parse("unpin").unwrap();
        assert!(matches!(unpin.as_slice(), [Command::Unpin]));
        assert_eq!(unpin[0].to_string(), "unpin");
        assert!(matches!(
            parse(&unpin[0].to_string()).as_deref(),
            Ok([Command::Unpin])
        ));
        assert!(!pin[0].accepts_count() && !unpin[0].accepts_count());
    }

    #[test]
    fn parses_update_and_update_tab() {
        let update = parse("update").unwrap();
        assert!(matches!(update.as_slice(), [Command::UpdateLibrary]));
        assert_eq!(update[0].to_string(), "update");
        let tab = parse("update tab").unwrap();
        assert!(matches!(tab.as_slice(), [Command::UpdateTab]));
        assert_eq!(tab[0].to_string(), "update tab");
        assert!(matches!(
            parse(&tab[0].to_string()).as_deref(),
            Ok([Command::UpdateTab])
        ));
        assert!(parse("update all").is_err());
    }

    #[test]
    fn parses_jukebox_song_settings() {
        assert!(matches!(
            parse("jukeboxsongsettings").as_deref(),
            Ok([Command::JukeboxSongSettings])
        ));
    }

    #[test]
    fn eligible_commands_accept_count() {
        let eligible = [
            Command::Move(MoveMode::Down, MoveAmount::default()),
            Command::Move(MoveMode::Up, MoveAmount::Extreme),
            Command::Queue,
            Command::PlayNext,
            Command::Next,
            Command::Previous,
            Command::Seek(SeekDirection::Relative(1000)),
            Command::VolumeUp(1),
            Command::VolumeDown(5),
            Command::Jump(JumpMode::Next),
            Command::Jump(JumpMode::Previous),
        ];
        for cmd in eligible {
            assert!(cmd.accepts_count(), "{cmd:?} should accept a count");
        }
    }

    #[test]
    fn one_shot_and_menu_commands_ignore_count() {
        let ineligible = [
            Command::Open(TargetMode::Selected),
            Command::Goto(GotoMode::Album),
            Command::Play,
            Command::TogglePlay,
            Command::Stop,
            Command::Repeat(None),
            Command::Shuffle(None),
            Command::Help,
            Command::Back,
            Command::Delete,
            Command::Quit,
            Command::Focus("queue".into()),
            Command::Shift(ShiftMode::Up, None),
        ];
        for cmd in ineligible {
            assert!(!cmd.accepts_count(), "{cmd:?} should ignore a count");
        }
    }

    #[test]
    fn binding_accepts_count_only_if_all_commands_do() {
        let space = [
            Command::Queue,
            Command::Move(MoveMode::Down, MoveAmount::default()),
        ];
        let dot = [
            Command::PlayNext,
            Command::Move(MoveMode::Down, MoveAmount::default()),
        ];
        let open = [Command::Open(TargetMode::Selected)];
        assert!(space.iter().all(Command::accepts_count));
        assert!(dot.iter().all(Command::accepts_count));
        assert!(!open.iter().all(Command::accepts_count));
    }

    #[test]
    fn scaled_multiplies_seek_and_volume() {
        // `Command` has no `PartialEq`, so match on the collapsed variants.
        assert!(matches!(
            Command::Seek(SeekDirection::Relative(-1000)).scaled(3),
            Some(Command::Seek(SeekDirection::Relative(-3000)))
        ));
        assert!(matches!(
            Command::VolumeUp(5).scaled(4),
            Some(Command::VolumeUp(20))
        ));
        assert!(matches!(
            Command::VolumeDown(2).scaled(3),
            Some(Command::VolumeDown(6))
        ));
    }

    #[test]
    fn parses_lyrics_seek() {
        assert!(matches!(
            parse("lyricsseek").as_deref(),
            Ok([Command::LyricsSeek])
        ));
    }

    #[test]
    fn scaled_saturates_instead_of_overflowing() {
        assert!(matches!(
            Command::VolumeUp(u16::MAX).scaled(9999),
            Some(Command::VolumeUp(u16::MAX))
        ));
        assert!(matches!(
            Command::Seek(SeekDirection::Relative(i32::MIN)).scaled(9999),
            Some(Command::Seek(SeekDirection::Relative(i32::MIN)))
        ));
    }

    #[test]
    fn scaled_is_none_for_repeat_by_execution_commands() {
        // These skip/move commands are collapsed elsewhere or must genuinely
        // repeat, so they have no scaled form.
        assert!(Command::Next.scaled(3).is_none());
        assert!(Command::Previous.scaled(3).is_none());
        assert!(Command::Queue.scaled(3).is_none());
        assert!(
            Command::Move(MoveMode::Down, MoveAmount::default())
                .scaled(3)
                .is_none()
        );
        assert!(
            Command::Seek(SeekDirection::Absolute(1000))
                .scaled(3)
                .is_none()
        );
    }
}
