use cursive::theme::BaseColor::*;
use cursive::theme::Color::*;
use cursive::theme::PaletteColor::*;
use cursive::theme::*;
use log::warn;

use crate::config::ConfigTheme;

macro_rules! load_color {
    ( $theme: expr, $member: ident, $default: expr ) => {
        $theme
            .as_ref()
            .and_then(|t| t.$member.clone())
            .and_then(|c| Color::parse(c.as_ref()))
            .unwrap_or_else(|| {
                warn!(
                    "Failed to parse color in \"{}\", falling back to default",
                    stringify!($member)
                );
                $default
            })
    };
}

pub fn load(theme_cfg: &Option<ConfigTheme>) -> Theme {
    let mut palette = Palette::default();
    let borders = BorderStyle::Simple;

    palette[Background] = load_color!(theme_cfg, background, TerminalDefault);
    palette[View] = load_color!(theme_cfg, background, TerminalDefault);
    palette[Primary] = load_color!(theme_cfg, primary, TerminalDefault);
    palette[Secondary] = load_color!(theme_cfg, secondary, Dark(Blue));
    palette[TitlePrimary] = load_color!(theme_cfg, title, Dark(Red));
    palette[HighlightText] = load_color!(theme_cfg, highlight, Dark(White));
    palette[Highlight] = load_color!(theme_cfg, highlight_bg, Dark(Red));
    palette[HighlightInactive] = load_color!(theme_cfg, highlight_inactive_bg, Dark(Blue));
    palette.set_color("playing", load_color!(theme_cfg, playing, Dark(Blue)));
    palette.set_color(
        "playing_selected",
        load_color!(theme_cfg, playing_selected, Light(Blue)),
    );
    palette.set_color(
        "playing_bg",
        load_color!(theme_cfg, playing_bg, TerminalDefault),
    );
    palette.set_color("error", load_color!(theme_cfg, error, TerminalDefault));
    palette.set_color("error_bg", load_color!(theme_cfg, error_bg, Dark(Red)));
    palette.set_color(
        "statusbar_progress",
        load_color!(theme_cfg, statusbar_progress, Dark(Blue)),
    );
    palette.set_color(
        "statusbar_progress_bg",
        load_color!(theme_cfg, statusbar_progress_bg, Light(Black)),
    );
    palette.set_color("statusbar", load_color!(theme_cfg, statusbar, Dark(Yellow)));
    palette.set_color(
        "statusbar_bg",
        load_color!(theme_cfg, statusbar_bg, TerminalDefault),
    );
    palette.set_color("cmdline", load_color!(theme_cfg, cmdline, TerminalDefault));
    palette.set_color(
        "cmdline_bg",
        load_color!(theme_cfg, cmdline_bg, TerminalDefault),
    );
    palette.set_color(
        "search_match",
        load_color!(theme_cfg, search_match, Light(Red)),
    );

    Theme {
        shadow: false,
        palette,
        borders,
    }
}
