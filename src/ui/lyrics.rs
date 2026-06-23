use std::sync::{Arc, RwLock};

use cursive::style::Style;
use cursive::theme::{ColorStyle, ColorType, Effect};
use cursive::utils::markup::StyledString;
use cursive::views::{DummyView, LinearLayout, ResizedView, ScrollView, TextContent, TextView};
use cursive::{Cursive, Printer, Vec2, View};

use crate::command::Command;
use crate::commands::CommandResult;
use crate::lyrics::{LyricsDisplayStatus, LyricsManager, TrackLyrics};
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::theme::{DEFAULT_LYRICS_BACKGROUND, DEFAULT_LYRICS_HIGHLIGHT};
use crate::traits::ViewExt;

const HEADER_HEIGHT: usize = 6;

struct LyricsRenderState {
    last_active_line: Option<usize>,
    last_scroll_line: Option<usize>,
    last_track_uri: Option<String>,
    scroll_area_size: Vec2,
}

impl Default for LyricsRenderState {
    fn default() -> Self {
        Self {
            last_active_line: None,
            last_scroll_line: None,
            last_track_uri: None,
            scroll_area_size: Vec2::new(0, 0),
        }
    }
}

pub struct LyricsView {
    queue: Arc<Queue>,
    spotify: Spotify,
    manager: Arc<LyricsManager>,
    header: LinearLayout,
    lyrics_scroll: RwLock<ScrollView<TextView>>,
    track_title: TextContent,
    track_artists: TextContent,
    track_album: TextContent,
    track_lyrics: TextContent,
    render_state: RwLock<LyricsRenderState>,
}

impl LyricsView {
    pub fn new(queue: Arc<Queue>, spotify: Spotify, manager: Arc<LyricsManager>) -> Self {
        let track_title = TextContent::new("");
        let track_artists = TextContent::new("");
        let track_album = TextContent::new("");
        let track_lyrics = TextContent::new("");

        let header = LinearLayout::vertical()
            .child(ResizedView::with_full_width(ResizedView::with_fixed_height(
                2,
                DummyView,
            )))
            .child(
                TextView::new_with_content(track_title.clone())
                    .center()
                    .style(Effect::Bold),
            )
            .child(TextView::new_with_content(track_artists.clone()).center())
            .child(
                TextView::new_with_content(track_album.clone())
                    .center()
                    .style(Effect::Italic),
            )
            .child(DummyView);

        let lyrics_scroll = RwLock::new(ScrollView::new(
            TextView::new_with_content(track_lyrics.clone()).center(),
        ));

        Self {
            queue,
            spotify,
            manager,
            header,
            lyrics_scroll,
            track_title,
            track_artists,
            track_album,
            track_lyrics,
            render_state: RwLock::new(LyricsRenderState::default()),
        }
    }

    fn sync_display(&self, printer: &Printer<'_, '_>) {
        let state = self.manager.state().read().unwrap().clone();
        let mut render_state = self.render_state.write().unwrap();

        if render_state.last_track_uri.as_ref() != state.track_uri.as_ref() {
            render_state.last_track_uri = state.track_uri.clone();
            render_state.last_active_line = None;
            render_state.last_scroll_line = None;
        }

        self.track_title.set_content(state.title);
        self.track_artists.set_content(state.artists);
        self.track_album.set_content(state.album);

        match state.status {
            LyricsDisplayStatus::Loading => {
                self.track_lyrics.set_content("Loading lyrics...");
                render_state.last_scroll_line = None;
            }
            LyricsDisplayStatus::Idle if state.message.is_empty() => {
                self.track_lyrics
                    .set_content("No track is currently playing.");
                render_state.last_scroll_line = None;
            }
            LyricsDisplayStatus::Ready => {
                if let Some(lyrics) = state.lyrics {
                    self.render_lyrics(&lyrics, &mut render_state, printer);
                }
            }
            _ => {
                self.track_lyrics.set_content(state.message);
                render_state.last_scroll_line = None;
            }
        }
    }

    fn render_lyrics(
        &self,
        lyrics: &TrackLyrics,
        render_state: &mut LyricsRenderState,
        printer: &Printer<'_, '_>,
    ) {
        if lyrics.is_synced() {
            let position_ms = self.spotify.get_current_progress().as_millis() as u64;
            let active_line = lyrics.active_line_index(position_ms);
            let highlight_style = lyrics_highlight_style(printer);
            let content = format_synced_lyrics(lyrics, active_line, highlight_style);

            if render_state.last_active_line != Some(active_line) {
                self.track_lyrics.set_content(content);
                render_state.last_active_line = Some(active_line);
                render_state.last_scroll_line = None;
            }

            self.scroll_to_line(active_line, render_state);
        } else {
            self.track_lyrics.set_content(lyrics.as_plain_text());
            render_state.last_scroll_line = None;
        }
    }

    fn scroll_to_line(&self, line: usize, render_state: &mut LyricsRenderState) {
        if render_state.last_scroll_line == Some(line) {
            return;
        }

        let viewport_height = render_state.scroll_area_size.y;
        if viewport_height == 0 {
            return;
        }

        let target = line.saturating_sub(viewport_height / 2);
        self.lyrics_scroll
            .write()
            .unwrap()
            .set_offset((0, target));
        render_state.last_scroll_line = Some(line);
    }
}

fn lyrics_highlight_style(printer: &Printer<'_, '_>) -> Style {
    let foreground = printer
        .theme
        .palette
        .custom("lyrics_highlight")
        .copied()
        .unwrap_or_else(|| {
            cursive::theme::Color::parse(DEFAULT_LYRICS_HIGHLIGHT)
                .expect("valid default lyrics highlight color")
        });
    let background = printer
        .theme
        .palette
        .custom("lyrics_background")
        .copied()
        .unwrap_or_else(|| {
            cursive::theme::Color::parse(DEFAULT_LYRICS_BACKGROUND)
                .expect("valid default lyrics background color")
        });

    Style::from_color_style(ColorStyle::new(
        ColorType::Color(foreground),
        ColorType::Color(background),
    ))
    .combine(Effect::Bold)
}

fn format_synced_lyrics(
    lyrics: &TrackLyrics,
    active_line: usize,
    highlight_style: Style,
) -> StyledString {
    let mut content = StyledString::default();

    for (index, line) in lyrics.lines.iter().enumerate() {
        if index > 0 {
            content.append_plain("\n");
        }

        let style = if index == active_line {
            highlight_style
        } else {
            Style::primary()
        };
        content.append_styled(&line.text, style);
    }

    content
}

impl View for LyricsView {
    fn draw(&self, printer: &Printer<'_, '_>) {
        self.manager
            .sync_current_track(self.queue.get_current().clone());
        self.sync_display(printer);

        let header_printer = printer
            .offset((0, 0))
            .cropped((printer.size.x, HEADER_HEIGHT.min(printer.size.y)));
        self.header.draw(&header_printer);

        if printer.size.y > HEADER_HEIGHT {
            let lyrics_printer = printer
                .offset((0, HEADER_HEIGHT))
                .cropped((printer.size.x, printer.size.y - HEADER_HEIGHT));
            self.lyrics_scroll.read().unwrap().draw(&lyrics_printer);
        }
    }

    fn layout(&mut self, size: Vec2) {
        let header_size = Vec2::new(size.x, HEADER_HEIGHT.min(size.y));
        self.header.layout(header_size);

        let mut render_state = self.render_state.write().unwrap();
        if size.y > HEADER_HEIGHT {
            let scroll_size = Vec2::new(size.x, size.y - HEADER_HEIGHT);
            self.lyrics_scroll.write().unwrap().layout(scroll_size);
            render_state.scroll_area_size = scroll_size;
        } else {
            render_state.scroll_area_size = Vec2::new(0, 0);
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, constraint.y)
    }

    fn on_event(&mut self, event: cursive::event::Event) -> cursive::event::EventResult {
        if self.header.on_event(event.relativized((0, 0))).is_consumed() {
            return cursive::event::EventResult::Consumed(None);
        }

        self.lyrics_scroll
            .write()
            .unwrap()
            .on_event(event.relativized((0, HEADER_HEIGHT)))
    }

    fn call_on_any(
        &mut self,
        selector: &cursive::view::Selector,
        callback: cursive::event::AnyCb<'_>,
    ) {
        self.header.call_on_any(selector, callback);
        self.lyrics_scroll
            .write()
            .unwrap()
            .call_on_any(selector, callback);
    }

    fn take_focus(
        &mut self,
        source: cursive::direction::Direction,
    ) -> Result<cursive::event::EventResult, cursive::view::CannotFocus> {
        self.lyrics_scroll.write().unwrap().take_focus(source)
    }
}

impl ViewExt for LyricsView {
    fn title(&self) -> String {
        "Lyrics".to_string()
    }

    fn title_sub(&self) -> String {
        let state_guard = self.manager.state();
        let state = state_guard.read().unwrap();
        if state.status == LyricsDisplayStatus::Ready
            && let Some(lyrics) = &state.lyrics
            && lyrics.is_synced()
        {
            return format!("Synced · {}", lyrics.provider);
        }
        String::new()
    }

    fn on_command(
        &mut self,
        _s: &mut Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Next | Command::Previous => Ok(CommandResult::Ignored),
            _ => Ok(CommandResult::Ignored),
        }
    }
}
