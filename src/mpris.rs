use std::{error::Error, future::pending};
use zbus::{dbus_interface, ConnectionBuilder};

use crate::{
    events::EventManager,
    spotify::{PlayerEvent, Spotify, VOLUME_PERCENT},
};

struct MprisRoot {}

#[dbus_interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[dbus_interface(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[dbus_interface(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[dbus_interface(property)]
    fn has_tracklist(&self) -> bool {
        true
    }

    #[dbus_interface(property)]
    fn identity(&self) -> &str {
        "ncspot"
    }

    #[dbus_interface(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["spotify".to_string()]
    }

    #[dbus_interface(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct MprisPlayer {
    event: EventManager,
    spotify: Spotify,
}

#[dbus_interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    #[dbus_interface(property)]
    fn playback_status(&self) -> &str {
        match self.spotify.get_current_status() {
            PlayerEvent::Playing(_) | PlayerEvent::FinishedTrack => "Playing",
            PlayerEvent::Paused(_) => "Paused",
            _ => "Stopped",
        }
    }

    #[dbus_interface(property)]
    fn playback_rate(&self) -> f64 {
        1.0
    }

    #[dbus_interface(property)]
    fn volume(&self) -> f64 {
        self.spotify.volume() as f64 / 65535_f64
    }

    #[dbus_interface(property)]
    fn set_volume(&self, volume: f64) {
        log::info!("set volume: {volume}");
        if (0.0..=1.0).contains(&volume) {
            let vol = (VOLUME_PERCENT as f64) * volume * 100.0;
            self.spotify.set_volume(vol as u16);
            self.event.trigger();
        }
    }
}

pub async fn serve(
    event: EventManager,
    spotify: Spotify,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let root = MprisRoot {};
    let player = MprisPlayer { event, spotify };

    let _conn = ConnectionBuilder::session()?
        .name("org.mpris.MediaPlayer2.ncspot")?
        .serve_at("/org/mpris/MediaPlayer2", root)?
        .serve_at("/org/mpris/MediaPlayer2", player)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
