use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use dbus::arg::{Variant, RefArg};
use dbus::tree::{Access};
use dbus_tokio::AConnection;
use dbus_tokio::tree::{AFactory, ATree, ATreeServer};

use tokio::reactor::Handle;
use tokio::runtime::current_thread::Runtime;
use futures::Stream;

use queue::Queue;
use spotify::{PlayerEvent, Spotify};

pub fn run_dbus_server(spotify: Arc<Spotify>, queue: Arc<Mutex<Queue>>) {
    let conn = Rc::new(dbus::Connection::get_private(dbus::BusType::Session)
        .expect("Failed to connect to dbus"));
    conn.register_name("org.mpris.MediaPlayer2.ncspot", dbus::NameFlag::ReplaceExisting as u32)
        .expect("Failed to register dbus player name");

    let f = AFactory::new_afn::<()>();

    let property_canquit = f.property::<bool, _>("CanQuit", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false); // TODO
            Ok(())
        });

    let property_canraise = f.property::<bool, _>("CanRaise", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false);
            Ok(())
        });

    let property_cansetfullscreen = f.property::<bool, _>("CanSetFullscreen", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false);
            Ok(())
        });

    let property_hastracklist = f.property::<bool, _>("HasTrackList", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false); // TODO
            Ok(())
        });

    let property_identity = f.property::<String, _>("Identity", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append("ncspot".to_string());
            Ok(())
        });

    let property_urischemes = f.property::<Vec<String>, _>("SupportedUriSchemes", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(vec!["spotify".to_string()]);
            Ok(())
        });

    let property_mimetypes = f.property::<Vec<String>, _>("SupportedMimeTypes", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(Vec::new() as Vec<String>);
            Ok(())
        });

    // https://specifications.freedesktop.org/mpris-spec/latest/Media_Player.html
    let interface = f.interface("org.mpris.MediaPlayer", ())
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
                let status = match spotify.get_current_status() {
                    PlayerEvent::Playing => "Playing",
                    PlayerEvent::Paused => "Paused",
                    _ => "Stopped"
                }.to_string();
                iter.append(status);
                Ok(())
            })
    };

    let property_loopstatus = f.property::<String, _>("LoopStatus", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append("None".to_string()); // TODO
            Ok(())
        });

    let property_metadata = {
        let queue = queue.clone();
        f.property::<HashMap<String, Variant<Box<RefArg>>>, _>("Metadata", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                let mut hm: HashMap<String, Variant<Box<RefArg>>> = HashMap::new();

                let queue = queue.lock().expect("could not lock queue");
                let track = queue.get_current();

                hm.insert("mpris:trackid".to_string(), Variant(Box::new(
                    track.map(|t| format!("spotify:track:{}", t.id.to_base62())).unwrap_or("".to_string())
                )));
                hm.insert("mpris:length".to_string(), Variant(Box::new(
                    track.map(|t| t.duration * 1_000_000).unwrap_or(0)
                )));
                hm.insert("mpris:artUrl".to_string(), Variant(Box::new(
                    track.map(|t| t.cover_url.clone()).unwrap_or("".to_string())
                )));

                hm.insert("xesam:album".to_string(), Variant(Box::new(
                    track.map(|t| t.album.clone()).unwrap_or("".to_string())
                )));
                hm.insert("xesam:albumArtist".to_string(), Variant(Box::new(
                    track.map(|t| t.album_artists.join(", ")).unwrap_or("".to_string())
                )));
                hm.insert("xesam:artist".to_string(), Variant(Box::new(
                    track.map(|t| t.artists.join(", ")).unwrap_or("".to_string())
                )));
                hm.insert("xesam:discNumber".to_string(), Variant(Box::new(
                    track.map(|t| t.disc_number).unwrap_or(0)
                )));
                hm.insert("xesam:title".to_string(), Variant(Box::new(
                    track.map(|t| t.title.clone()).unwrap_or("".to_string())
                )));
                hm.insert("xesam:trackNumber".to_string(), Variant(Box::new(
                    track.map(|t| t.track_number).unwrap_or(0)
                )));
                hm.insert("xesam:url".to_string(), Variant(Box::new(
                    track.map(|t| t.url.clone()).unwrap_or("".to_string())
                )));

                iter.append(hm);
                Ok(())
            })
    };

    let property_position = {
        let spotify = spotify.clone();
        f.property::<i64, _>("Position", ())
            .access(Access::Read)
            .on_get(move |iter, _| {
                iter.append(spotify.get_current_progress().as_micros() as i64);
                Ok(())
            })
    };

    let property_volume = f.property::<f64, _>("Volume", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_rate = f.property::<f64, _>("Rate", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_minrate = f.property::<f64, _>("MinimumRate", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_maxrate = f.property::<f64, _>("MaximumRate", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(1.0);
            Ok(())
        });

    let property_canplay = f.property::<bool, _>("CanPlay", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_canpause = f.property::<bool, _>("CanPause", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_canseek = f.property::<bool, _>("CanSeek", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(false); // TODO
            Ok(())
        });

    let property_cancontrol = f.property::<bool, _>("CanControl", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_cangonext = f.property::<bool, _>("CanGoNext", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let property_cangoprevious = f.property::<bool, _>("CanGoPrevious", ())
        .access(Access::Read)
        .on_get(|iter, _| {
            iter.append(true);
            Ok(())
        });

    let method_playpause = {
        let spotify = spotify.clone();
        f.amethod("PlayPause", (), move |m| {
            spotify.toggleplayback();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_play = {
        let spotify = spotify.clone();
        f.amethod("Play", (), move |m| {
            spotify.play();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_pause = {
        let spotify = spotify.clone();
        f.amethod("Pause", (), move |m| {
            spotify.pause();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_stop = {
        let spotify = spotify.clone();
        f.amethod("Stop", (), move |m| {
            spotify.stop();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_next = {
        let queue = queue.clone();
        f.amethod("Next", (), move |m| {
            queue.lock().expect("failed to lock queue").next();
            Ok(vec![m.msg.method_return()])
        })
    };

    let method_previous = {
        let queue = queue.clone();
        f.amethod("Previous", (), move |m| {
            queue.lock().expect("failed to lock queue").previous();
            Ok(vec![m.msg.method_return()])
        })
    };

    // TODO: Seek, SetPosition, Shuffle, OpenUri (?)

    // https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html
    let interface_player = f.interface("org.mpris.MediaPlayer2.Player", ())
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
        .add_m(method_playpause)
        .add_m(method_play)
        .add_m(method_pause)
        .add_m(method_stop)
        .add_m(method_next)
        .add_m(method_previous);

    let tree = f.tree(ATree::new())
        .add(f.object_path("/org/mpris/MediaPlayer2", ()).introspectable()
            .add(interface)
            .add(interface_player)
        );

    tree.set_registered(&conn, true).expect("failed to register tree");

    let mut rt = Runtime::new().unwrap();
    let aconn = AConnection::new(conn.clone(), Handle::default(), &mut rt).unwrap();
    let server = ATreeServer::new(conn.clone(), &tree, aconn.messages().unwrap());

    let server = server.for_each(|m| {
        warn!("Unhandled dbus message: {:?}", m);
        Ok(())
    });
    rt.block_on(server).unwrap();
    rt.run().unwrap();
}
