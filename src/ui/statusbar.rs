use std::sync::Arc;

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::Printer;
use unicode_width::UnicodeWidthStr;

use queue::Queue;
use spotify::{PlayerEvent, Spotify};

pub struct StatusBar {
    queue: Arc<Queue>,
    spotify: Arc<Spotify>,
    last_size: Vec2,
}

impl StatusBar {
    pub fn new(queue: Arc<Queue>, spotify: Arc<Spotify>) -> StatusBar {
        StatusBar {
            queue: queue,
            spotify: spotify,
            last_size: Vec2::new(0, 0),
        }
    }
}

impl View for StatusBar {
    fn draw(&self, printer: &Printer<'_, '_>) {
        if printer.size.x == 0 {
            return;
        }

        let style_bar = ColorStyle::new(
            ColorType::Color(*printer.theme.palette.custom("statusbar_progress").unwrap()),
            ColorType::Palette(PaletteColor::Background),
        );
        let style = ColorStyle::new(
            ColorType::Color(*printer.theme.palette.custom("statusbar").unwrap()),
            ColorType::Color(*printer.theme.palette.custom("statusbar_bg").unwrap()),
        );

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
            let elapsed_ms = elapsed.as_secs() as u32 * 1000 + elapsed.subsec_millis();

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
                    (((printer.size.x as u32) * elapsed_ms) / t.duration) as usize;
                printer.print((0, 0), &format!("{}{}", "=".repeat(duration_width), ">"));
            });
        } else {
            printer.with_color(style_bar, |printer| {
                printer.print((0, 0), &"—".repeat(printer.size.x));
            });
        }
    }

    fn layout(&mut self, size: Vec2) {
        self.last_size = size;
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, 2)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Event::Mouse {
            offset,
            position,
            event
        } = event {
            let position = position - offset;

            if position.y == 0 {
                if event == MouseEvent::WheelUp {
                    self.spotify.seek_relative(-500);
                }

                if event == MouseEvent::WheelDown {
                    self.spotify.seek_relative(500);
                }

                if event == MouseEvent::Press(MouseButton::Left) ||
                    event == MouseEvent::Hold(MouseButton::Left)
                {
                    if let Some(ref t) = self.queue.get_current() {
                        let f: f32 = position.x as f32 / self.last_size.x as f32;
                        let new = t.duration as f32 * f;
                        self.spotify.seek(new as u32);
                    }

                }
            } else {
                if event == MouseEvent::Press(MouseButton::Left) {
                    self.queue.toggleplayback();
                }
            }

            EventResult::Consumed(None)
        } else {
            EventResult::Ignored
        }
    }
}
