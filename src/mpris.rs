use std::{error::Error, future::pending};
use zbus::{dbus_interface, ConnectionBuilder};

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

pub async fn serve() -> Result<(), Box<dyn Error + Sync + Send>> {
    let root = MprisRoot {};

    let _conn = ConnectionBuilder::session()?
        .name("org.mpris.MediaPlayer2.ncspot")?
        .serve_at("/org/mpris/MediaPlayer2", root)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
