use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use dbus::arg::{RefArg, Variant};
use dbus::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged;
use dbus::tree::{Access, Factory};
use dbus::{Path, SignalArgs};

use crate::album::Album;
use crate::playlist::Playlist;
use crate::queue::{Queue, RepeatSetting, Playable};
use crate::spotify::{PlayerEvent, Spotify, URIType};
use crate::track::Track;

type Metadata = HashMap<String, Variant<Box<dyn RefArg>>>;
struct MprisState(String, Option<Playable>);

fn get_playbackstatus(spotify: Arc<Spotify>) -> String {
    match spotify.get_current_status() {
        PlayerEvent::Playing => "Playing",
        PlayerEvent::Paused => "Paused",
        _ => "Stopped",
    }
    .to_string()
}

fn get_metadata(track: Option<Playable>) -> Metadata {
    let mut hm: Metadata = HashMap::new();
    let track = track.as_ref();

    hm.insert(
        "mpris:trackid".to_string(),
        Variant(Box::new(
            track
                .map(|t| {
                    format!(
                        "spotify:track:{}",
                        t.id.clone().unwrap_or_else(|| "0".to_string())
                    )
                })
                .unwrap_or_default(),
        )),
    );
    hm.insert(
        "mpris:length".to_string(),
        Variant(Box::new(i64::from(
            track.map(|t| t.duration * 1_000).unwrap_or(0),
        ))),
    );
    hm.insert(
        "mpris:artUrl".to_string(),
        Variant(Box::new(
            track.map(|t| t.cover_url.clone()).unwrap_or_default(),
        )),
    );

    hm.insert(
        "xesam:album".to_string(),
        Variant(Box::new(track.map(|t| t.album.clone()).unwrap_or_default())),
    );
    hm.insert(
        "xesam:albumArtist".to_string(),
        Variant(Box::new(
            track.map(|t| t.album_artists.clone()).unwrap_or_default(),
        )),
    );
    hm.insert(
        "xesam:artist".to_string(),
        Variant(Box::new(
            track.map(|t| t.artists.clone()).unwrap_or_default(),
        )),
    );
    hm.insert(
        "xesam:discNumber".to_string(),
        Variant(Box::new(track.map(|t| t.disc_number).unwrap_or(0))),
    );
    hm.insert(
        "xesam:title".to_string(),
        Variant(Box::new(track.map(|t| t.title.clone()).unwrap_or_default())),
    );
    hm.insert(
        "xesam:trackNumber".to_string(),
        Variant(Box::new(track.map(|t| t.track_number).unwrap_or(0) as i32)),
    );
    hm.insert(
        "xesam:url".to_string(),
        Variant(Box::new(track.map(|t| t.url.clone()).unwrap_or_default())),
    );

    hm
}

fn run_dbus_server(spotify: Arc<Spotify>, queue: Arc<Queue>, rx: mpsc::Receiver<MprisState>) {
    let conn = Rc::new(
        dbus::Connection::get_private(dbus::BusType::Session).expect("Failed to connect to dbus"),
    );
    conn.register_name(
        "org.mpris.MediaPlayer2.ncspot",
        dbus::NameFlag::ReplaceExisting as u32,
    )
    .expect("Failed to register dbus player name");

    let f = Factory::new_fn::<()>();

    let property_canquit = f
        .property::<bool, _>("CanQuit", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false); // TODO
            Ok(())
        });

    let property_canraise = f
        .property::<bool, _>("CanRaise", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false);
            Ok(())
        });

    let property_cansetfullscreen = f
        .property::<bool, _>("CanSetFullscreen", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false);
            Ok(())
        });

    let property_hastracklist = f
        .property::<bool, _>("HasTrackList", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false); // TODO
            Ok(())
        });

    let property_identity = f
        .property::<String, _>("Identity", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append("ncspot".to_string());
            Ok(())
        });

    let property_urischemes = f
        .property::<Vec<String>, _>("SupportedUriSchemes", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(vec!["spotify".to_string()]);
            Ok(())
        });

    let property_mimetypes = f
        .property::<Vec<String>, _>("SupportedMimeTypes", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(Vec::new() as Vec<String>);
            Ok(())
        });

    // https://specifications.freedesktop.org/mpris-spec/latest/Media_Player.html
    let interface = f
        .interface("org.mpris.MediaPlayer2", ())
        .add_p(property_canquit)
        .add_p(property_canraise)
        .add_p(property_cansetfullscreen)
        .add_p(property_hastracklist)
        .add_p(property_identity)
        .add_p(property_urischemes)
        .add_p(property_mimetypes);

    let property_playbackstatus = {
        let spotify = spotify.clone();
        f.property::<String, _>("PlaybackStatus", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                let status = get_playbackstatus(spotify.clone());
                iter.append(status);
                Ok(())
            })
    };

    let property_loopstatus = {
        let queue = queue.clone();
        f.property::<String, _>("LoopStatus", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                iter.append(
                    match queue.get_repeat() {
                        RepeatSetting::None => "None",
                        RepeatSetting::RepeatTrack => "Track",
                        RepeatSetting::RepeatPlaylist => "Playlist",
                    }
                    .to_string(),
                );
                Ok(())
            })
    };

    let property_metadata = {
        let queue = queue.clone();
        f.property::<HashMap<String, Variant<Box<dyn RefArg>>>, _>("Metadata", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                let hm = get_metadata(queue.clone().get_current());

                iter.append(hm);
                Ok(())
            })
    };

    let property_position = {
        let spotify = spotify.clone();
        let progress = spotify.get_current_progress();
        f.property::<i64, _>("Position", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                iter.append(progress.as_micros() as i64);
                Ok(())
            })
    };

    let property_volume = f
        .property::<f64, _>("Volume", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_rate = f
        .property::<f64, _>("Rate", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_minrate = f
        .property::<f64, _>("MinimumRate", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_maxrate = f
        .property::<f64, _>("MaximumRate", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_canplay = f
        .property::<bool, _>("CanPlay", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_canpause = f
        .property::<bool, _>("CanPause", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_canseek = f
        .property::<bool, _>("CanSeek", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false); // TODO
            Ok(())
        });

    let property_cancontrol = f
        .property::<bool, _>("CanControl", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_cangonext = f
        .property::<bool, _>("CanGoNext", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_cangoprevious = f
        .property::<bool, _>("CanGoPrevious", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_shuffle = {
        let queue = queue.clone();
        f.property::<bool, _>("Shuffle", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                iter.append(queue.get_shuffle());
                Ok(())
            })
    };

    let property_cangoforward = f
        .property::<bool, _>("CanGoForward", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_canrewind = f
        .property::<bool, _>("CanRewind", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let method_playpause = {
        let spotify = spotify.clone();
        f.method("PlayPause", (), move |m| {
            spotify.toggleplayback();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_play = {
        let spotify = spotify.clone();
        f.method("Play", (), move |m| {
            spotify.play();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_pause = {
        let spotify = spotify.clone();
        f.method("Pause", (), move |m| {
            spotify.pause();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_stop = {
        let spotify = spotify.clone();
        f.method("Stop", (), move |m| {
            spotify.stop();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_next = {
        let queue = queue.clone();
        f.method("Next", (), move |m| {
            queue.next(true);
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_previous = {
        let spotify = spotify.clone();
        let queue = queue.clone();
        f.method("Previous", (), move |m| {
            if spotify.get_current_progress() < Duration::from_secs(5) {
                queue.previous();
            } else {
                spotify.seek(0);
            }
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_forward = {
        let spotify = spotify.clone();
        f.method("Forward", (), move |m| {
            spotify.seek_relative(5000);
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_rewind = {
        let spotify = spotify.clone();
        f.method("Rewind", (), move |m| {
            spotify.seek_relative(-5000);
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_openuri = {
        f.method("OpenUri", (), move |m| {
            let uri_data: Option<&str> = m.msg.get1();
            let uri = match uri_data {
                Some(s) => s,
                None => "",
            };
            let id = &uri[uri.rfind(':').unwrap_or(0) + 1..uri.len()];
            let uri_type = URIType::from_uri(uri);
            match uri_type {
                Some(URIType::Album) => {
                    if let Some(a) = spotify.album(&id) {
                        if let Some(t) = &Album::from(&a).tracks {
                            queue.clear();
                            let index = queue.append_next(t.iter().collect());
                            queue.play(index, false, false)
                        }
                    }
                }
                Some(URIType::Track) => {
                    if let Some(t) = spotify.track(&id) {
                        queue.clear();
                        queue.append(Playable::Track(Track::from(&t)));
                        queue.play(0, false, false)
                    }
                }
                Some(URIType::Playlist) => {
                    if let Some(p) = spotify.playlist(&id) {
                        let mut playlist = Playlist::from(&p);
                        let spotify = spotify.clone();
                        playlist.load_tracks(spotify);
                        if let Some(t) = &playlist.tracks {
                            queue.clear();
                            let index = queue.append_next(t.iter().collect());
                            queue.play(index, false, false)
                        }
                    }
                }
                Some(URIType::Artist) => {}
                None => {}
            }
            Ok(vec![m.msg.method_return()])
        })
    };

    // TODO: Seek, SetPosition (?)

    // https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html
    let interface_player = f
        .interface("org.mpris.MediaPlayer2.Player", ())
        .add_p(property_playbackstatus)
        .add_p(property_loopstatus)
        .add_p(property_metadata)
        .add_p(property_position)
        .add_p(property_volume)
        .add_p(property_rate)
        .add_p(property_minrate)
        .add_p(property_maxrate)
        .add_p(property_canplay)
        .add_p(property_canpause)
        .add_p(property_canseek)
        .add_p(property_cancontrol)
        .add_p(property_cangonext)
        .add_p(property_cangoprevious)
        .add_p(property_shuffle)
        .add_p(property_cangoforward)
        .add_p(property_canrewind)
        .add_m(method_playpause)
        .add_m(method_play)
        .add_m(method_pause)
        .add_m(method_stop)
        .add_m(method_next)
        .add_m(method_previous)
        .add_m(method_forward)
        .add_m(method_rewind)
        .add_m(method_openuri);

    let tree = f.tree(()).add(
        f.object_path("/org/mpris/MediaPlayer2", ())
            .introspectable()
            .add(interface)
            .add(interface_player),
    );

    tree.set_registered(&conn, true)
        .expect("failed to register tree");

    conn.add_handler(tree);
    loop {
        if let Some(m) = conn.incoming(200).next() {
            warn!("Unhandled dbus message: {:?}", m);
        }

        if let Ok(state) = rx.try_recv() {
            let mut changed: PropertiesPropertiesChanged = Default::default();
            debug!(
                "mpris PropertiesChanged: status {}, track: {:?}",
                state.0, state.1
            );

            changed.interface_name = "org.mpris.MediaPlayer2.Player".to_string();
            changed.changed_properties.insert(
                "Metadata".to_string(),
                Variant(Box::new(get_metadata(state.1))),
            );

            changed
                .changed_properties
                .insert("PlaybackStatus".to_string(), Variant(Box::new(state.0)));

            conn.send(
                changed.to_emit_message(&Path::new("/org/mpris/MediaPlayer2".to_string()).unwrap()),
            )
            .unwrap();
        }
    }
}

#[derive(Clone)]
pub struct MprisManager {
    tx: mpsc::Sender<MprisState>,
    queue: Arc<Queue>,
    spotify: Arc<Spotify>,
}

impl MprisManager {
    pub fn new(spotify: Arc<Spotify>, queue: Arc<Queue>) -> Self {
        let (tx, rx) = mpsc::channel::<MprisState>();

        {
            let spotify = spotify.clone();
            let queue = queue.clone();
            std::thread::spawn(move || {
                run_dbus_server(spotify.clone(), queue.clone(), rx);
            });
        }

        MprisManager { tx, queue, spotify }
    }

    pub fn update(&self) {
        let status = get_playbackstatus(self.spotify.clone());
        let track = self.queue.get_current();
        self.tx.send(MprisState(status, track)).unwrap();
    }
}
