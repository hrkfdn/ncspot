#![cfg(feature = "share_clipboard")]
use arboard::Clipboard;
use std::error::Error;

#[cfg(feature = "share_selection")]
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

pub fn read_share() -> Result<String, Box<dyn Error>> {
    let mut ctx = Clipboard::new()?;

    #[cfg(feature = "share_selection")]
    return Ok(ctx.get().clipboard(LinuxClipboardKind::Primary).text()?);

    #[cfg(not(feature = "share_selection"))]
    return Ok(ctx.get_text()?);
}

pub fn write_share(url: String) -> Result<(), Box<dyn Error>> {
    let mut ctx = Clipboard::new()?;

    #[cfg(feature = "share_selection")]
    return Ok(ctx.set().clipboard(LinuxClipboardKind::Primary).text(url)?);

    #[cfg(not(feature = "share_selection"))]
    return Ok(ctx.set_text(url)?);
}
