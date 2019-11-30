use queue::RepeatSetting;
use std::collections::HashMap;
use std::iter::FromIterator;

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
pub enum SeekDirection {
    Relative(i32),
    Absolute(u32),
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
    Play,
    UpdateLibrary,
    Save,
    SaveQueue,
    Delete,
    Focus(String),
    Seek(SeekDirection),
    Repeat(Option<RepeatSetting>),
    Shuffle(Option<bool>),
    Share(TargetMode),
    Back,
    Open(TargetMode),
    Goto(GotoMode),
    Move(MoveMode, Option<i32>),
    Shift(ShiftMode, Option<i32>),
    Search(String),
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
        "search" => args.get(0).map(|query| Command::Search(query.to_string())),
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
            let amount = args.get(1).and_then(|amount| amount.parse().ok());

            args.get(0)
                .and_then(|direction| match *direction {
                    "up" => Some(MoveMode::Up),
                    "down" => Some(MoveMode::Down),
                    "left" => Some(MoveMode::Left),
                    "right" => Some(MoveMode::Right),
                    _ => None,
                })
                .map(|mode| Command::Move(mode, amount))
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
        "seek" => args.get(0).and_then(|arg| match arg.chars().nth(0) {
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
        "focus" => args.get(0).map(|target| Command::Focus(target.to_string())),
        "save" => args.get(0).map(|target| match *target {
            "queue" => Command::SaveQueue,
            _ => Command::Save,
        }),
        _ => None,
    }
}
