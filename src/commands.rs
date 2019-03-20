use std::collections::HashMap;
use std::sync::Arc;

use cursive::event::{Event, Key};
use cursive::Cursive;

use playlists::Playlist;
use queue::Queue;
use spotify::Spotify;
use track::Track;
use ui::layout::Layout;
use ui::listview::ListView;
use ui::search::SearchView;

pub struct CommandManager {
    commands:
        HashMap<String, Box<dyn Fn(&mut Cursive, Vec<String>) -> Result<Option<String>, String>>>,
    aliases: HashMap<String, String>,
}

impl CommandManager {
    pub fn new() -> CommandManager {
        CommandManager {
            commands: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn register<S: Into<String>>(
        &mut self,
        name: S,
        aliases: Vec<S>,
        cb: Box<dyn Fn(&mut Cursive, Vec<String>) -> Result<Option<String>, String>>,
    ) {
        let name = name.into();
        for a in aliases {
            self.aliases.insert(a.into(), name.clone());
        }
        self.commands.insert(name, cb);
    }

    pub fn register_all(&mut self, spotify: Arc<Spotify>, queue: Arc<Queue>) {
        self.register(
            "quit",
            vec!["q", "x"],
            Box::new(move |s, _args| {
                s.quit();
                Ok(None)
            }),
        );

        {
            let queue = queue.clone();
            self.register(
                "stop",
                Vec::new(),
                Box::new(move |_s, _args| {
                    queue.stop();
                    Ok(None)
                }),
            );
        }

        {
            let queue = queue.clone();
            self.register(
                "previous",
                Vec::new(),
                Box::new(move |_s, _args| {
                    queue.previous();
                    Ok(None)
                }),
            );
        }

        {
            let queue = queue.clone();
            self.register(
                "next",
                Vec::new(),
                Box::new(move |_s, _args| {
                    queue.next();
                    Ok(None)
                }),
            );
        }

        {
            let queue = queue.clone();
            self.register(
                "clear",
                Vec::new(),
                Box::new(move |_s, _args| {
                    queue.clear();
                    Ok(None)
                }),
            );
        }

        {
            let spotify = spotify.clone();
            self.register(
                "search",
                Vec::new(),
                Box::new(move |s, args| {
                    s.call_on_id("main", |v: &mut Layout| {
                        v.set_view("search");
                    });
                    s.call_on_id("search", |v: &mut SearchView| {
                        if args.len() >= 1 {
                            v.run_search(args.join(" "), spotify.clone());
                        }
                    });
                    Ok(None)
                }),
            );
        }

        {
            self.register(
                "playlists",
                vec!["lists"],
                Box::new(move |s, _args| {
                    s.call_on_id("main", |v: &mut Layout| {
                        v.set_view("playlists");
                    });
                    Ok(None)
                }),
            );
        }

        self.register(
            "log",
            Vec::new(),
            Box::new(move |s, _args| {
                s.call_on_id("main", |v: &mut Layout| {
                    v.set_view("log");
                });
                Ok(None)
            }),
        );

        self.register(
            "move",
            Vec::new(),
            Box::new(move |s, args| {
                if args.len() < 1 {
                    return Err("Missing direction (up, down, left, right)".to_string());
                }

                let dir = args.get(0).unwrap();

                let amount: i32 = args
                    .get(1)
                    .unwrap_or(&"1".to_string())
                    .parse()
                    .map_err(|e| format!("{:?}", e))?;

                if dir == "up" || dir == "down" {
                    let dir = if dir == "up" { -1 } else { 1 };
                    s.call_on_id("queue_list", |v: &mut ListView<Track>| {
                        v.move_focus(dir * amount);
                    });
                    s.call_on_id("list", |v: &mut ListView<Track>| {
                        v.move_focus(dir * amount);
                    });
                    s.call_on_id("list", |v: &mut ListView<Playlist>| {
                        v.move_focus(dir * amount);
                    });
                    s.on_event(Event::Refresh);
                    return Ok(None);
                }

                if dir == "left" || dir == "right" {
                    return Ok(None);
                }

                Err(format!("Unrecognized direction: {}", dir))
            }),
        );

        {
            let queue = queue.clone();
            self.register(
                "queue",
                Vec::new(),
                Box::new(move |s, args| {
                    if let Some(arg) = args.get(0) {
                        if arg != "selected" {
                            return Err("".into());
                        }
                    } else {
                        s.call_on_id("main", |v: &mut Layout| {
                            v.set_view("queue");
                        });
                        return Ok(None);
                    }

                    {
                        let queue = queue.clone();
                        s.call_on_id("list", |v: &mut ListView<Track>| {
                            v.with_selected(Box::new(move |t| {
                                queue.append(t);
                            }));
                        });
                    }

                    {
                        let queue = queue.clone();
                        s.call_on_id("list", |v: &mut ListView<Playlist>| {
                            v.with_selected(Box::new(move |pl| {
                                for track in pl.tracks.iter() {
                                    queue.append(track);
                                }
                            }));
                        });
                    }

                    Ok(None)
                }),
            );
        }

        {
            let queue = queue.clone();
            self.register(
                "play",
                vec!["pause", "toggle", "toggleplay", "toggleplayback"],
                Box::new(move |s, args| {
                    if let Some(arg) = args.get(0) {
                        if arg != "selected" {
                            return Err("".into());
                        }
                    } else {
                        queue.toggleplayback();
                        return Ok(None);
                    }

                    {
                        let queue = queue.clone();
                        s.call_on_id("queue_list", |v: &mut ListView<Track>| {
                            queue.play(v.get_selected_index());
                        });
                    }

                    {
                        let queue = queue.clone();
                        s.call_on_id("list", |v: &mut ListView<Track>| {
                            v.with_selected(Box::new(move |t| {
                                let index = queue.append_next(t);
                                queue.play(index);
                            }));
                        });
                    }

                    {
                        let queue = queue.clone();
                        s.call_on_id("list", |v: &mut ListView<Playlist>| {
                            v.with_selected(Box::new(move |pl| {
                                let indices: Vec<usize> =
                                    pl.tracks.iter().map(|t| queue.append_next(t)).collect();
                                if let Some(i) = indices.get(0) {
                                    queue.play(*i)
                                }
                            }));
                        });
                    }

                    Ok(None)
                }),
            );
        }

        {
            let queue = queue.clone();
            self.register(
                "delete",
                Vec::new(),
                Box::new(move |s, args| {
                    if let Some(arg) = args.get(0) {
                        if arg != "selected" {
                            return Err("".into());
                        }
                    } else {
                        return Err("".into());
                    }

                    {
                        let queue = queue.clone();
                        s.call_on_id("queue_list", |v: &mut ListView<Track>| {
                            queue.remove(v.get_selected_index());
                        });
                    }

                    Ok(None)
                }),
            );
        }
    }

    fn handle_aliases(&self, name: &String) -> String {
        if let Some(s) = self.aliases.get(name) {
            self.handle_aliases(s)
        } else {
            name.clone()
        }
    }

    pub fn handle(&self, s: &mut Cursive, cmd: String) {
        let components: Vec<String> = cmd.split(' ').map(|s| s.to_string()).collect();

        let result = if let Some(cb) = self.commands.get(&self.handle_aliases(&components[0])) {
            cb(s, components[1..].to_vec())
        } else {
            Err("Unknown command.".to_string())
        };

        // TODO: handle non-error output as well
        if let Err(e) = result {
            s.call_on_id("main", |v: &mut Layout| {
                v.set_error(e);
            });
        }
    }

    pub fn register_keybinding<'a, E: Into<cursive::event::Event>, S: Into<String>>(
        this: Arc<Self>,
        cursive: &'a mut Cursive,
        event: E,
        command: S,
    ) {
        let cmd = command.into();
        cursive.add_global_callback(event, move |s| {
            this.handle(s, cmd.clone());
        });
    }

    pub fn register_keybindings<'a>(
        this: Arc<Self>,
        cursive: &'a mut Cursive,
        keybindings: Option<HashMap<String, String>>,
    ) {
        let mut kb = Self::default_keybindings();
        kb.extend(keybindings.unwrap_or(HashMap::new()));

        for (k, v) in kb {
            Self::register_keybinding(this.clone(), cursive, Self::parse_keybinding(k), v);
        }
    }

    fn default_keybindings() -> HashMap<String, String> {
        let mut kb = HashMap::new();

        kb.insert("q".into(), "quit".into());
        kb.insert("P".into(), "toggle".into());
        kb.insert("S".into(), "stop".into());
        kb.insert("<".into(), "previous".into());
        kb.insert(">".into(), "next".into());
        kb.insert("c".into(), "clear".into());
        kb.insert(" ".into(), "queue selected".into());
        kb.insert("Enter".into(), "play selected".into());
        kb.insert("d".into(), "delete selected".into());
        kb.insert("/".into(), "search".into());

        kb.insert("F1".into(), "queue".into());
        kb.insert("F2".into(), "search".into());
        kb.insert("F3".into(), "playlists".into());
        kb.insert("F9".into(), "log".into());

        kb.insert("Up".into(), "move up".into());
        kb.insert("Down".into(), "move down".into());
        kb.insert("Left".into(), "move left".into());
        kb.insert("Right".into(), "move right".into());
        kb.insert("PageUp".into(), "move up 5".into());
        kb.insert("PageDown".into(), "move down 5".into());
        kb.insert("k".into(), "move up".into());
        kb.insert("j".into(), "move down".into());
        kb.insert("h".into(), "move left".into());
        kb.insert("l".into(), "move right".into());

        kb
    }

    fn parse_keybinding(kb: String) -> cursive::event::Event {
        match kb.as_ref() {
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
}
