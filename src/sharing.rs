#![cfg(feature = "share_clipboard")]
use std::env;

#[cfg(all(feature = "wl-clipboard-rs", target_os = "linux"))]
use {
    std::io::Read,
    wl_clipboard_rs::{
        copy,
        copy::{Options, Source},
        paste,
        paste::{get_contents, Error, Seat},
    },
};

#[cfg(feature = "share_selection")]
use clipboard::{x11_clipboard, x11_clipboard::X11ClipboardContext};
#[cfg(all(
    feature = "share_selection",
    all(feature = "wl-clipboard-rs", target_os = "linux")
))]
use wl_clipboard_rs::utils::{is_primary_selection_supported, PrimarySelectionCheckError};

#[cfg(not(feature = "share_selection"))]
use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;

fn is_wayland() -> bool {
    fn session_type() -> String {
        env::var("XDG_SESSION_TYPE").unwrap_or_default()
    }
    fn wl_display() -> String {
        env::var("WAYLAND_DISPLAY").unwrap_or_default()
    }
    fn gdk_backend() -> String {
        env::var("GDK_BACKEND").unwrap_or_default()
    }
    fn current_desktop() -> String {
        env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
    }
    current_desktop() != "GNOME"
        && (session_type().as_str() == "wayland"
            || !wl_display().is_empty()
            || gdk_backend() == "wayland")
}

#[cfg(not(feature = "share_selection"))]
pub fn read_share() -> Option<String> {
    if is_wayland() {
        #[allow(unused_mut, unused_assignments)]
        let mut string = None;
        #[cfg(all(feature = "wl-clipboard-rs", target_os = "linux"))]
        {
            //use wayland clipboard
            let result = get_contents(
                paste::ClipboardType::Regular,
                Seat::Unspecified,
                paste::MimeType::Text,
            );
            match result {
                Ok((mut pipe, _)) => {
                    let mut contents = vec![];
                    pipe.read_to_end(&mut contents).ok();
                    string = Some(String::from_utf8_lossy(&contents).to_string())
                }
                Err(Error::NoSeats) | Err(Error::ClipboardEmpty) | Err(Error::NoMimeType) => {
                    //The clipboard is empty or doesn't contain text, nothing to worry about.
                    string = None
                }
                Err(err) => {
                    eprintln!("{err}");
                    string = None
                }
            }
        }
        string
    } else {
        //use x11 clipboard
        ClipboardProvider::new()
            .and_then(|mut ctx: ClipboardContext| ctx.get_contents())
            .ok()
    }
}

#[cfg(feature = "share_selection")]
pub fn read_share() -> Option<String> {
    if is_wayland() {
        #[allow(unused_mut, unused_assignments)]
        let mut string = None;
        #[cfg(all(feature = "wl-clipboard-rs", target_os = "linux"))]
        {
            //use wayland clipboard
            string = match is_primary_selection_supported() {
                Ok(_supported) => {
                    let result = get_contents(
                        paste::ClipboardType::Primary,
                        Seat::Unspecified,
                        paste::MimeType::Text,
                    );
                    match result {
                        Ok((mut pipe, _)) => {
                            let mut contents = vec![];
                            pipe.read_to_end(&mut contents).ok();
                            Some(String::from_utf8_lossy(&contents).to_string())
                        }
                        Err(Error::NoSeats)
                        | Err(Error::ClipboardEmpty)
                        | Err(Error::NoMimeType) => {
                            //The clipboard is empty or doesn't contain text, nothing to worry about.
                            None
                        }
                        Err(err) => {
                            eprintln!("{}", err);
                            None
                        }
                    }
                }
                Err(PrimarySelectionCheckError::NoSeats) => {
                    // Impossible to give a definitive result. Primary selection may or may not be
                    // supported.

                    // The required protocol (data-control version 2) is there, but there are no seats.
                    // Unfortunately, at least one seat is needed to check for the primary clipboard support.
                    None
                }
                Err(PrimarySelectionCheckError::MissingProtocol { .. }) => {
                    // The data-control protocol (required for wl-clipboard-rs operation) is not
                    // supported by the compositor.
                    None
                }
                Err(err) => {
                    eprintln!("{}", err);
                    None
                }
            }
        }
        string
    } else {
        //use x11 clipboard
        ClipboardProvider::new()
            .and_then(|mut ctx: X11ClipboardContext<x11_clipboard::Primary>| ctx.get_contents())
            .ok()
    }
}

#[cfg(not(feature = "share_selection"))]
pub fn write_share(url: String) -> Option<()> {
    if is_wayland() {
        #[allow(unused_mut, unused_assignments)]
        let mut option = None;
        #[cfg(all(feature = "wl-clipboard-rs", target_os = "linux"))]
        {
            //use wayland clipboard
            let opts = Options::new();
            option = opts
                .copy(
                    Source::Bytes(url.into_bytes().into()),
                    copy::MimeType::Autodetect,
                )
                .ok()
        }
        option
    } else {
        //use x11 clipboard
        ClipboardProvider::new()
            .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url))
            .ok()
    }
}

#[cfg(feature = "share_selection")]
pub fn write_share(url: String) -> Option<()> {
    if is_wayland() {
        #[allow(unused_mut, unused_assignments)]
        let mut option = None;
        #[cfg(all(feature = "wl-clipboard-rs", target_os = "linux"))]
        {
            //use wayland clipboard
            let mut opts = Options::new();
            opts.clipboard(copy::ClipboardType::Primary);
            option = opts
                .copy(
                    Source::Bytes(url.into_bytes().into()),
                    copy::MimeType::Autodetect,
                )
                .ok()
        }
        option
    } else {
        //use x11 clipboard
        ClipboardProvider::new()
            .and_then(|mut ctx: X11ClipboardContext<x11_clipboard::Primary>| ctx.set_contents(url))
            .ok()
    }
}
