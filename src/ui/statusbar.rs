use std::sync::Arc;

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::Printer;
use unicode_width::UnicodeWidthStr;

use crate::library::Library;
use crate::model::playable::Playable;
use crate::queue::{Queue, RepeatSetting};
use crate::spotify::{PlayerEvent, Spotify};
use crate::utils::ms_to_hms;

pub struct StatusBar {
    queue: Arc<Queue>,
    spotify: Spotify,
    library: Arc<Library>,
    last_size: Vec2,
}

impl StatusBar {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> StatusBar {
        let spotify = queue.get_spotify();

        StatusBar {
            queue,
            spotify,
            library,
            last_size: Vec2::new(0, 0),
        }
    }

    fn use_nerdfont(&self) -> bool {
        self.library.cfg.values().use_nerdfont.unwrap_or(false)
    }

    fn playback_indicator(&self) -> &str {
        let status = self.spotify.get_current_status();
        let nerdfont = self.use_nerdfont();
        let flipped = self
            .library
            .cfg
            .values()
            .flip_status_indicators
            .unwrap_or(false);

        const NF_PLAY: &str = "\u{f04b} ";
        const NF_PAUSE: &str = "\u{f04c} ";
        const NF_STOP: &str = "\u{f04d} ";
        let indicators = match (nerdfont, flipped) {
            (false, false) => ("▶ ", "▮▮", "◼ "),
            (false, true) => ("▮▮", "▶ ", "▶ "),
            (true, false) => (NF_PLAY, NF_PAUSE, NF_STOP),
            (true, true) => (NF_PAUSE, NF_PLAY, NF_PLAY),
        };

        match status {
            PlayerEvent::Playing(_) => indicators.0,
            PlayerEvent::Paused(_) => indicators.1,
            PlayerEvent::Stopped | PlayerEvent::FinishedTrack => indicators.2,
        }
    }

    fn volume_display(&self) -> String {
        format!(
            " [{}%]",
            (self.spotify.volume() as f64 / 65535_f64 * 100.0).round() as u16
        )
    }

    fn format_track(&self, t: &Playable) -> String {
        let format = self
            .library
            .cfg
            .values()
            .statusbar_format
            .clone()
            .unwrap_or_else(|| "%artists - %title".to_string());
        Playable::format(t, &format, &self.library)
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
        let style_bar_bg = ColorStyle::new(
            ColorType::Color(
                *printer
                    .theme
                    .palette
                    .custom("statusbar_progress_bg")
                    .unwrap(),
            ),
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

        printer.with_color(style, |printer| {
            printer.print((1, 1), self.playback_indicator());
        });

        let updating = if !*self.library.is_done.read().unwrap() {
            if self.use_nerdfont() {
                "\u{f04e6} "
            } else {
                "[U] "
            }
        } else {
            ""
        };

        let repeat = if self.use_nerdfont() {
            match self.queue.get_repeat() {
                RepeatSetting::None => "",
                RepeatSetting::RepeatPlaylist => "\u{f0456} ",
                RepeatSetting::RepeatTrack => "\u{f0458} ",
            }
        } else {
            match self.queue.get_repeat() {
                RepeatSetting::None => "",
                RepeatSetting::RepeatPlaylist => "[R] ",
                RepeatSetting::RepeatTrack => "[R1] ",
            }
        };

        let shuffle = if self.queue.get_shuffle() {
            if self.use_nerdfont() {
                "\u{f049d} "
            } else {
                "[Z] "
            }
        } else {
            ""
        };

        let volume = self.volume_display();

        printer.with_color(style_bar_bg, |printer| {
            printer.print((0, 0), &"┉".repeat(printer.size.x));
        });

        let elapsed = self.spotify.get_current_progress();
        let elapsed_ms = elapsed.as_millis() as u32;

        let formatted_elapsed = ms_to_hms(elapsed.as_millis().try_into().unwrap_or(0));

        let playback_duration_status = match self.queue.get_current() {
            Some(ref t) => format!("{} / {}", formatted_elapsed, t.duration_str()),
            None => "".to_string(),
        };

        let right = updating.to_string()
            + repeat
            + shuffle
            // + saved
            + &playback_duration_status
            + &volume;
        let offset = HAlign::Right.get_offset(right.width(), printer.size.x);

        printer.with_color(style, |printer| {
            if let Some(ref t) = self.queue.get_current() {
                printer.print((4, 1), &self.format_track(t));
            }
            printer.print((offset, 1), &right);
        });

        if let Some(t) = self.queue.get_current() {
            printer.with_color(style_bar, |printer| {
                let duration_width =
                    (((printer.size.x as u32) * elapsed_ms) / t.duration()) as usize;
                printer.print((0, 0), &"━".repeat(duration_width + 1));
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
            event,
        } = event
        {
            let position = position - offset;
            let volume_len = self.volume_display().len();

            if position.y == 0 {
                if event == MouseEvent::WheelUp {
                    self.spotify.seek_relative(-500);
                }

                if event == MouseEvent::WheelDown {
                    self.spotify.seek_relative(500);
                }

                if event == MouseEvent::Press(MouseButton::Left) {
                    if let Some(playable) = self.queue.get_current() {
                        let f: f32 = position.x as f32 / self.last_size.x as f32;
                        let new = playable.duration() as f32 * f;
                        self.spotify.seek(new as u32);
                    }
                }
            } else if self.last_size.x - position.x < volume_len {
                if event == MouseEvent::WheelUp {
                    let volume = self
                        .spotify
                        .volume()
                        .saturating_add(crate::spotify::VOLUME_PERCENT);

                    self.spotify.set_volume(volume);
                }

                if event == MouseEvent::WheelDown {
                    let volume = self
                        .spotify
                        .volume()
                        .saturating_sub(crate::spotify::VOLUME_PERCENT);

                    self.spotify.set_volume(volume);
                }
            } else if event == MouseEvent::Press(MouseButton::Left) {
                self.queue.toggleplayback();
            }

            EventResult::Consumed(None)
        } else {
            EventResult::Ignored
        }
    }
}
