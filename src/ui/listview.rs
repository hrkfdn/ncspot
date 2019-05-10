use std::cmp::{max, min};
use std::sync::{Arc, RwLock};

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::view::ScrollBase;
use cursive::{Cursive, Printer, Rect, Vec2};
use unicode_width::UnicodeWidthStr;

use clipboard::{ClipboardContext, ClipboardProvider};
use commands::CommandResult;
use library::Library;
use queue::Queue;
use track::Track;
use traits::{IntoBoxedViewExt, ListItem, ViewExt};
use ui::album::AlbumView;
use ui::artist::ArtistView;

pub type Paginator<I> = Box<Fn(Arc<RwLock<Vec<I>>>) + Send + Sync>;
pub struct Pagination<I: ListItem> {
    max_content: Arc<RwLock<Option<usize>>>,
    callback: Arc<RwLock<Option<Paginator<I>>>>,
    busy: Arc<RwLock<bool>>,
}

impl<I: ListItem> Default for Pagination<I> {
    fn default() -> Self {
        Pagination {
            max_content: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            busy: Arc::new(RwLock::new(false)),
        }
    }
}

// TODO: figure out why deriving Clone doesn't work
impl<I: ListItem> Clone for Pagination<I> {
    fn clone(&self) -> Self {
        Pagination {
            max_content: self.max_content.clone(),
            callback: self.callback.clone(),
            busy: self.busy.clone(),
        }
    }
}

impl<I: ListItem> Pagination<I> {
    pub fn clear(&mut self) {
        *self.max_content.write().unwrap() = None;
        *self.callback.write().unwrap() = None;
    }
    pub fn set(&mut self, max_content: usize, callback: Paginator<I>) {
        *self.max_content.write().unwrap() = Some(max_content);
        *self.callback.write().unwrap() = Some(callback);
    }

    fn max_content(&self) -> Option<usize> {
        *self.max_content.read().unwrap()
    }

    fn is_busy(&self) -> bool {
        *self.busy.read().unwrap()
    }

    fn call(&self, content: &Arc<RwLock<Vec<I>>>) {
        let pagination = self.clone();
        let content = content.clone();
        if !self.is_busy() {
            *self.busy.write().unwrap() = true;
            std::thread::spawn(move || {
                let cb = pagination.callback.read().unwrap();
                if let Some(ref cb) = *cb {
                    debug!("calling paginator!");
                    cb(content);
                    *pagination.busy.write().unwrap() = false;
                }
            });
        }
    }
}

pub struct ListView<I: ListItem> {
    content: Arc<RwLock<Vec<I>>>,
    last_content_len: usize,
    selected: usize,
    last_size: Vec2,
    scrollbar: ScrollBase,
    queue: Arc<Queue>,
    library: Arc<Library>,
    pagination: Pagination<I>,
}

impl<I: ListItem> ListView<I> {
    pub fn new(content: Arc<RwLock<Vec<I>>>, queue: Arc<Queue>, library: Arc<Library>) -> Self {
        Self {
            content,
            last_content_len: 0,
            selected: 0,
            last_size: Vec2::new(0, 0),
            scrollbar: ScrollBase::new(),
            queue,
            library,
            pagination: Pagination::default(),
        }
    }

    pub fn get_pagination(&self) -> &Pagination<I> {
        &self.pagination
    }

    fn can_paginate(&self) -> bool {
        if let Some(max) = self.get_pagination().max_content() {
            trace!(
                "pagination: total: {}, current: {}",
                max,
                self.last_content_len
            );
            if max > self.last_content_len {
                return true;
            }
        }
        false
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected
    }

    pub fn move_focus_to(&mut self, target: usize) {
        let len = self.content.read().unwrap().len().saturating_sub(1);
        self.selected = min(target, len);
        self.scrollbar.scroll_to(self.selected);
    }

    pub fn move_focus(&mut self, delta: i32) {
        let new = self.selected as i32 + delta;
        self.move_focus_to(max(new, 0) as usize);
    }

    fn attempt_play_all_tracks(&self) -> bool {
        let content = self.content.read().unwrap();
        let any = &(*content) as &dyn std::any::Any;
        if let Some(tracks) = any.downcast_ref::<Vec<Track>>() {
            let tracks: Vec<&Track> = tracks.iter().collect();
            let index = self.queue.append_next(tracks);
            self.queue.play(index + self.selected, true);
            true
        } else {
            false
        }
    }
}

impl<I: ListItem> View for ListView<I> {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let content = self.content.read().unwrap();

        self.scrollbar.draw(printer, |printer, i| {
            let item = &content[i];

            let style = if self.selected == i {
                let fg = if item.is_playing(self.queue.clone()) {
                    *printer.theme.palette.custom("playing").unwrap()
                } else {
                    PaletteColor::Tertiary.resolve(&printer.theme.palette)
                };
                ColorStyle::new(
                    ColorType::Color(fg),
                    ColorType::Palette(PaletteColor::Highlight),
                )
            } else if item.is_playing(self.queue.clone()) {
                ColorStyle::new(
                    ColorType::Color(*printer.theme.palette.custom("playing").unwrap()),
                    ColorType::Color(*printer.theme.palette.custom("playing_bg").unwrap()),
                )
            } else {
                ColorStyle::primary()
            };

            let left = item.display_left();
            let right = item.display_right(self.library.clone());

            // draw left string
            printer.with_color(style, |printer| {
                printer.print_hline((0, 0), printer.size.x, " ");
                printer.print((0, 0), &left);
            });

            // draw ".." to indicate a cut off string
            let max_length = printer.size.x.saturating_sub(right.width() + 1);
            if max_length < left.width() {
                let offset = max_length.saturating_sub(1);
                printer.with_color(style, |printer| {
                    printer.print((offset, 0), "..");
                });
            }

            // draw right string
            let offset = HAlign::Right.get_offset(right.width(), printer.size.x);

            printer.with_color(style, |printer| {
                printer.print((offset, 0), &right);
            });
        });
    }

    fn layout(&mut self, size: Vec2) {
        self.last_content_len = self.content.read().unwrap().len();
        self.last_size = size;
        self.scrollbar.set_heights(size.y, self.last_content_len);
    }

    fn needs_relayout(&self) -> bool {
        self.content.read().unwrap().len() != self.last_content_len
    }

    fn on_event(&mut self, e: Event) -> EventResult {
        match e {
            Event::Mouse {
                event: MouseEvent::WheelUp,
                ..
            } => self.move_focus(-3),
            Event::Mouse {
                event: MouseEvent::WheelDown,
                ..
            } => self.move_focus(3),
            Event::Mouse {
                event: MouseEvent::Press(MouseButton::Left),
                position,
                offset,
            } => {
                if self.scrollbar.scrollable()
                    && position.y > 0
                    && position.y <= self.last_size.y
                    && position
                        .checked_sub(offset)
                        .map(|p| self.scrollbar.start_drag(p, self.last_size.x))
                        .unwrap_or(false)
                {}
            }
            Event::Mouse {
                event: MouseEvent::Hold(MouseButton::Left),
                position,
                offset,
            } => {
                if self.scrollbar.scrollable() {
                    self.scrollbar.drag(position.saturating_sub(offset));
                }
            }
            Event::Mouse {
                event: MouseEvent::Release(MouseButton::Left),
                ..
            } => {
                self.scrollbar.release_grab();
            }
            _ => {
                return EventResult::Ignored;
            }
        }

        EventResult::Consumed(None)
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, self.content.read().unwrap().len())
    }

    fn important_area(&self, view_size: Vec2) -> Rect {
        if self.content.read().unwrap().len() > 0 {
            Rect::from((view_size.x, self.selected))
        } else {
            Rect::from((0, 0))
        }
    }
}

impl<I: ListItem + Clone> ViewExt for ListView<I> {
    fn on_command(
        &mut self,
        _s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        if cmd == "play" {
            self.queue.clear();

            if !self.attempt_play_all_tracks() {
                let mut content = self.content.write().unwrap();
                if let Some(item) = content.get_mut(self.selected) {
                    item.play(self.queue.clone());
                }
            }

            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "queue" {
            let mut content = self.content.write().unwrap();
            if let Some(item) = content.get_mut(self.selected) {
                item.queue(self.queue.clone());
            }
            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "save" {
            let mut item = {
                let content = self.content.read().unwrap();
                content.get(self.selected).cloned()
            };

            if let Some(item) = item.as_mut() {
                item.toggle_saved(self.library.clone());
            }
        }

        if cmd == "share" {
            let source = args.get(0);
            let url =
                source.and_then(|source| match source.as_str() {
                    "selected" => self.content.read().ok().and_then(|content| {
                        content.get(self.selected).and_then(ListItem::share_url)
                    }),
                    "current" => self.queue.get_current().and_then(|t| t.share_url()),
                    _ => None,
                });

            return if let Some(url) = url {
                ClipboardProvider::new()
                    .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url))
                    .ok();
                Ok(CommandResult::Consumed(None))
            } else {
                Ok(CommandResult::Ignored)
            };
        }

        if cmd == "move" {
            if let Some(dir) = args.get(0) {
                let amount: usize = args
                    .get(1)
                    .unwrap_or(&"1".to_string())
                    .parse()
                    .map_err(|e| format!("{:?}", e))?;

                let len = self.content.read().unwrap().len();

                if dir == "up" && self.selected > 0 {
                    self.move_focus(-(amount as i32));
                    return Ok(CommandResult::Consumed(None));
                }

                if dir == "down" {
                    if self.selected < len.saturating_sub(1) {
                        self.move_focus(amount as i32);
                        return Ok(CommandResult::Consumed(None));
                    } else if self.selected == len.saturating_sub(1) && self.can_paginate() {
                        self.pagination.call(&self.content);
                    }
                }
            }
        }

        if cmd == "open" {
            let mut content = self.content.write().unwrap();
            if let Some(item) = content.get_mut(self.selected) {
                let queue = self.queue.clone();
                let library = self.library.clone();
                if let Some(view) = item.open(queue, library) {
                    return Ok(CommandResult::View(view));
                }
            }
        }

        if cmd == "goto" {
            let mut content = self.content.write().unwrap();
            if let Some(item) = content.get_mut(self.selected) {
                let queue = self.queue.clone();
                let library = self.library.clone();
                let arg = args.get(0).cloned().unwrap_or_default();

                if arg == "album" {
                    if let Some(album) = item.album(queue.clone()) {
                        let view = AlbumView::new(queue, library, &album).as_boxed_view_ext();
                        return Ok(CommandResult::View(view));
                    }
                }

                if arg == "artist" {
                    if let Some(artist) = item.artist() {
                        let view = ArtistView::new(queue, library, &artist).as_boxed_view_ext();
                        return Ok(CommandResult::View(view));
                    }
                }
            }
        }

        Ok(CommandResult::Ignored)
    }
}
