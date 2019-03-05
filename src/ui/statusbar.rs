use std::sync::Arc;

use cursive::align::HAlign;
use cursive::theme::{ColorStyle, ColorType, Color, BaseColor};
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::Printer;
use unicode_width::UnicodeWidthStr;

use spotify::{PlayerStatus, Spotify};

pub struct StatusBar {
    spotify: Arc<Spotify>
}

impl StatusBar {
    pub fn new(spotify: Arc<Spotify>) -> StatusBar {
        StatusBar {
            spotify: spotify
        }
    }
}

impl View for StatusBar {
    fn draw(&self, printer: &Printer<'_, '_>) {
        if printer.size.x == 0 {
            return;
        }

        let front = ColorType::Color(Color::Dark(BaseColor::Black));
        let back = ColorType::Color(Color::Dark(BaseColor::Green));
        let style = ColorStyle::new(front, back);

        printer.print((0, 0), &vec![' '; printer.size.x].into_iter().collect::<String>());
        printer.with_color(style, |printer| {
            printer.print((0, 1), &vec![' '; printer.size.x].into_iter().collect::<String>());
        });

        let state_icon = match self.spotify.get_current_status() {
            PlayerStatus::Playing => " ▶ ",
            PlayerStatus::Paused => " ▮▮ ",
            PlayerStatus::Stopped => " ◼  ",
        }.to_string();

        printer.with_color(style, |printer| {
            printer.print((0, 1), &state_icon);
        });

        if let Some(ref t) = self.spotify.get_current_track() {
            let name = format!("{} - {}",
                t.artists.iter().map(|ref artist| artist.name.clone()).collect::<Vec<String>>().join(", "),
                t.name).to_string();

            let minutes = t.duration_ms / 60000;
            let seconds = (t.duration_ms % 60000) / 1000;
            let formatted_duration = format!("{:02}:{:02}", minutes, seconds);

            let elapsed = self.spotify.get_current_progress();
            let formatted_elapsed = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);

            let duration = format!("{} / {} ", formatted_elapsed, formatted_duration);
            let offset = HAlign::Right.get_offset(duration.width(), printer.size.x);

            printer.with_color(style, |printer| {
                printer.print((4, 1), &name);
                printer.print((offset, 1), &duration);
            });

            printer.with_color(ColorStyle::new(back, front), |printer| {
                printer.print_hline((0, 0), (((printer.size.x as u32) * (elapsed.as_millis() as u32)) / t.duration_ms) as usize, "=")
            });
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, 2)
    }
}
