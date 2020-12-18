#![cfg(feature = "share_clipboard")]

#[cfg(feature = "share_selection")]
use clipboard::x11_clipboard::{Primary, X11ClipboardContext};
#[cfg(not(feature = "share_selection"))]
use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;

#[cfg(not(feature = "share_selection"))]
pub fn read_share() -> Option<String> {
    ClipboardProvider::new()
        .and_then(|mut ctx: ClipboardContext| ctx.get_contents())
        .ok()
}

#[cfg(feature = "share_selection")]
pub fn read_share() -> Option<String> {
    ClipboardProvider::new()
        .and_then(|mut ctx: X11ClipboardContext<Primary>| ctx.get_contents())
        .ok()
}

#[cfg(not(feature = "share_selection"))]
pub fn write_share(url: String) -> Option<()> {
    ClipboardProvider::new()
        .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url))
        .ok()
}

#[cfg(feature = "share_selection")]
pub fn write_share(url: String) -> Option<()> {
    ClipboardProvider::new()
        .and_then(|mut ctx: X11ClipboardContext<Primary>| ctx.set_contents(url))
        .ok()
}
