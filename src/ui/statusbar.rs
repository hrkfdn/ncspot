use std::sync::Arc;

use cursive::align::HAlign;
use cursive::theme::ColorStyle;
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::Printer;
use unicode_width::UnicodeWidthStr;

use queue::Queue;
use spotify::{PlayerEvent, Spotify};

pub struct StatusBar {
    queue: Arc<Queue>,
    spotify: Arc<Spotify>,
}

impl StatusBar {
    pub fn new(queue: Arc<Queue>, spotify: Arc<Spotify>) -> StatusBar {
        StatusBar {
            queue: queue,
            spotify: spotify,
        }
    }
}

impl View for StatusBar {
    fn draw(&self, printer: &Printer<'_, '_>) {
        if printer.size.x == 0 {
            return;
        }

        let style_bar = ColorStyle::secondary();
        let style = ColorStyle::title_secondary();

        printer.print(
            (0, 0),
            &vec![' '; printer.size.x].into_iter().collect::<String>(),
        );
        printer.with_color(style, |printer| {
            printer.print(
                (0, 1),
                &vec![' '; printer.size.x].into_iter().collect::<String>(),
            );
        });

        let state_icon = match self.spotify.get_current_status() {
            PlayerEvent::Playing => "▶ ",
            PlayerEvent::Paused => "▮▮",
            PlayerEvent::Stopped | PlayerEvent::FinishedTrack => "◼ ",
        }
        .to_string();

        printer.with_color(style, |printer| {
            printer.print((0, 1), &state_icon);
        });

        if let Some(ref t) = self.queue.get_current() {
            let elapsed = self.spotify.get_current_progress();
            let formatted_elapsed = format!(
                "{:02}:{:02}",
                elapsed.as_secs() / 60,
                elapsed.as_secs() % 60
            );

            let duration = format!("{} / {} ", formatted_elapsed, t.duration_str());
            let offset = HAlign::Right.get_offset(duration.width(), printer.size.x);

            printer.with_color(style, |printer| {
                printer.print((4, 1), &t.to_string());
                printer.print((offset, 1), &duration);
            });

            printer.with_color(style_bar, |printer| {
                printer.print((0, 0), &"—".repeat(printer.size.x));
                let duration_width =
                    (((printer.size.x as u32) * (elapsed.as_secs() as u32)) / t.duration) as usize;
                printer.print((0, 0), &format!("{}{}", "=".repeat(duration_width), ">"));
            });
        } else {
            printer.with_color(style_bar, |printer| {
                printer.print((0, 0), &"—".repeat(printer.size.x));
            });
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, 2)
    }
}
