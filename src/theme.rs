use cursive::theme::Color::*;
use cursive::theme::PaletteColor::*;
use cursive::theme::*;

pub fn default() -> Theme {
    let mut palette = Palette::default();
    let borders = BorderStyle::None;

    palette[Background] = TerminalDefault;

    Theme {
        shadow: false,
        palette: palette,
        borders: borders,
    }
}
