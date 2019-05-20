#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PlaylistCommands {
    Update,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SeekInterval {
    Forward,
    Backwards,
    Custom(usize),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TargetMode {
    Current,
    Selected,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum MoveMode {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ShiftMode {
    Up,
    Down,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum GotoMode {
    Album,
    Artist,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Command {
    Quit,
    TogglePlay,
    Playlists(PlaylistCommands),
    Stop,
    Previous,
    Next,
    Clear,
    Queue,
    Play,
    Save,
    SaveQueue,
    Delete,
    Focus(String),
    Seek(SeekInterval),
    Repeat,
    Shuffle,
    Share(TargetMode),
    Back,
    Open,
    Goto(GotoMode),
    Move(MoveMode, Option<usize>),
    Shift(ShiftMode, Option<usize>),
    Search(String),
}
