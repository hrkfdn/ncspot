use cursive::theme::BaseColor::*;
use cursive::theme::Color::*;
use cursive::theme::PaletteColor::*;
use cursive::theme::*;

use config::Config;

macro_rules! load_color {
    ( $cfg: expr, $member: ident, $default: expr ) => {
        $cfg.theme
            .as_ref()
            .and_then(|t| t.$member.clone())
            .map(|c| Color::parse(c.as_ref()).expect(&format!("Failed to parse color \"{}\"", c)))
            .unwrap_or($default)
    };
}

pub fn load(cfg: &Config) -> Theme {
    let mut palette = Palette::default();
    let borders = BorderStyle::None;

    palette[Background] = load_color!(cfg, background, TerminalDefault);
    palette[View] = load_color!(cfg, background, TerminalDefault);
    palette[Primary] = load_color!(cfg, primary, TerminalDefault);
    palette[Secondary] = load_color!(cfg, secondary, Dark(Blue));
    palette[TitlePrimary] = load_color!(cfg, title, Dark(Red));
    palette[Tertiary] = load_color!(cfg, highlight, TerminalDefault);
    palette[Highlight] = load_color!(cfg, highlight_bg, Dark(Red));
    palette.set_color("playing", load_color!(cfg, playing, Dark(Blue)));
    palette.set_color("playing_bg", load_color!(cfg, playing_bg, TerminalDefault));
    palette.set_color("error", load_color!(cfg, error, TerminalDefault));
    palette.set_color("error_bg", load_color!(cfg, error_bg, Dark(Red)));
    palette.set_color(
        "statusbar_progress",
        load_color!(cfg, statusbar_progress, Dark(Blue)),
    );
    palette.set_color("statusbar", load_color!(cfg, statusbar, Dark(Yellow)));
    palette.set_color(
        "statusbar_bg",
        load_color!(cfg, statusbar_bg, TerminalDefault),
    );
    palette.set_color("cmdline", load_color!(cfg, cmdline, TerminalDefault));
    palette.set_color("cmdline_bg", load_color!(cfg, cmdline_bg, TerminalDefault));

    Theme {
        shadow: false,
        palette: palette,
        borders: borders,
    }
}
