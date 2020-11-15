use std::cmp::{max, min, Ordering};
use std::sync::{Arc, RwLock};

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::view::ScrollBase;
use cursive::{Cursive, Printer, Rect, Vec2};
use unicode_width::UnicodeWidthStr;

use crate::album::Album;
use crate::artist::Artist;
use crate::command::{Command, GotoMode, JumpMode, MoveAmount, MoveMode, TargetMode};
use crate::commands::CommandResult;
use crate::episode::Episode;
use crate::library::Library;
use crate::playable::Playable;
use crate::playlist::Playlist;
use crate::queue::Queue;
use crate::show::Show;
use crate::track::Track;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::album::AlbumView;
use crate::ui::artist::ArtistView;
use crate::ui::contextmenu::ContextMenu;
#[cfg(feature = "share_clipboard")]
use clipboard::{ClipboardContext, ClipboardProvider};
use regex::Regex;

pub type Paginator<I> = Box<dyn Fn(Arc<RwLock<Vec<I>>>) + Send + Sync>;

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
    search_query: String,
    search_indexes: Vec<usize>,
    search_selected_index: usize,
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
            search_query: String::new(),
            search_indexes: Vec::new(),
            search_selected_index: 0,
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

    pub fn get_indexes_of(&self, query: &str) -> Vec<usize> {
        let content = self.content.read().unwrap();
        content
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                i.display_left()
                    .to_lowercase()
                    .contains(&query[..].to_lowercase())
            })
            .map(|(i, _)| i)
            .collect()
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
            let tracks: Vec<Playable> = tracks
                .iter()
                .map(|track| Playable::Track(track.clone()))
                .collect();
            let index = self.queue.append_next(tracks);
            self.queue.play(index + self.selected, true, false);
            true
        } else {
            false
        }
    }

    pub fn remove(&self, index: usize) {
        let mut c = self.content.write().unwrap();
        c.remove(index);
    }
}

impl<I: ListItem> View for ListView<I> {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let content = self.content.read().unwrap();

        self.scrollbar.draw(printer, |printer, i| {
            // draw paginator after content
            if i == content.len() {
                let style = ColorStyle::secondary();

                let max = self.pagination.max_content().unwrap();
                let buf = format!("{} more items, scroll to load", max - i);
                printer.with_color(style, |printer| {
                    printer.print((0, 0), &buf);
                });
            } else {
                let item = &content[i];

                let style = if self.selected == i {
                    let fg = if item.is_playing(self.queue.clone()) {
                        *printer.theme.palette.custom("playing_selected").unwrap()
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
                let center = item.display_center(self.library.clone());
                let right = item.display_right(self.library.clone());
                let draw_center = !center.is_empty();

                // draw left string
                printer.with_color(style, |printer| {
                    printer.print_hline((0, 0), printer.size.x, " ");
                    printer.print((0, 0), &left);
                });

                // if line contains search query match, draw on top with
                // highlight color
                if self.search_indexes.contains(&i) {
                    let fg = *printer.theme.palette.custom("search_match").unwrap();
                    let matched_style = ColorStyle::new(fg, style.back);

                    let matches: Vec<(usize, usize)> = left
                        .to_lowercase()
                        .match_indices(&self.search_query)
                        .map(|i| (i.0, i.0 + i.1.len()))
                        .collect();

                    for m in matches {
                        printer.with_color(matched_style, |printer| {
                            printer.print((m.0, 0), &left[m.0..m.1]);
                        });
                    }
                }

                // left string cut off indicator
                let center_offset = printer.size.x / 2;
                let left_max_length = if draw_center {
                    center_offset.saturating_sub(1)
                } else {
                    printer.size.x.saturating_sub(right.width() + 1)
                };

                if left_max_length < left.width() {
                    let offset = left_max_length.saturating_sub(1);
                    printer.with_color(style, |printer| {
                        printer.print_hline((offset, 0), printer.size.x, " ");
                        printer.print((offset, 0), "..");
                    });
                }

                // draw center string
                if draw_center {
                    printer.with_color(style, |printer| {
                        printer.print((center_offset, 0), &center);
                    });

                    // center string cut off indicator
                    let max_length = printer.size.x.saturating_sub(right.width() + 1);
                    if max_length < center_offset + center.width() {
                        let offset = max_length.saturating_sub(1);
                        printer.with_color(style, |printer| {
                            printer.print((offset, 0), "..");
                        });
                    }
                }

                // draw right string
                let offset = HAlign::Right.get_offset(right.width(), printer.size.x);

                printer.with_color(style, |printer| {
                    printer.print((offset, 0), &right);
                });
            }
        });
    }

    fn layout(&mut self, size: Vec2) {
        let content_len = self.content.read().unwrap().len();

        // add 1 more row for paginator if we can paginate
        self.last_content_len = if self.can_paginate() {
            content_len + 1
        } else {
            content_len
        };

        self.last_size = size;
        self.scrollbar.set_heights(size.y, self.last_content_len);
    }

    fn needs_relayout(&self) -> bool {
        self.content.read().unwrap().len() != self.last_content_len
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, self.content.read().unwrap().len())
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

    fn important_area(&self, view_size: Vec2) -> Rect {
        if self.content.read().unwrap().len() > 0 {
            Rect::from((view_size.x, self.selected))
        } else {
            Rect::from((0, 0))
        }
    }
}

impl<I: ListItem + Clone> ViewExt for ListView<I> {
    fn on_command(&mut self, _s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Play => {
                self.queue.clear();

                if !self.attempt_play_all_tracks() {
                    let mut content = self.content.write().unwrap();
                    if let Some(item) = content.get_mut(self.selected) {
                        item.play(self.queue.clone());
                    }
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::PlayNext => {
                info!("played next");
                let mut content = self.content.write().unwrap();
                if let Some(item) = content.get_mut(self.selected) {
                    item.play_next(self.queue.clone());
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Queue => {
                let mut content = self.content.write().unwrap();
                if let Some(item) = content.get_mut(self.selected) {
                    item.queue(self.queue.clone());
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Save => {
                let mut item = {
                    let content = self.content.read().unwrap();
                    content.get(self.selected).cloned()
                };

                if let Some(item) = item.as_mut() {
                    item.save(self.library.clone());
                }
            }
            Command::Delete => {
                let mut item = {
                    let content = self.content.read().unwrap();
                    content.get(self.selected).cloned()
                };

                if let Some(item) = item.as_mut() {
                    item.unsave(self.library.clone());
                }
            }
            Command::Share(mode) => {
                let url = match mode {
                    TargetMode::Selected => self.content.read().ok().and_then(|content| {
                        content.get(self.selected).and_then(ListItem::share_url)
                    }),
                    TargetMode::Current => self
                        .queue
                        .get_current()
                        .and_then(|t| t.as_listitem().share_url()),
                };

                if let Some(url) = url {
                    #[cfg(feature = "share_clipboard")]
                    ClipboardProvider::new()
                        .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url))
                        .ok();
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Jump(mode) => match mode {
                JumpMode::Query(query) => {
                    self.search_query = query.to_lowercase();
                    self.search_indexes = self.get_indexes_of(query);
                    self.search_selected_index = 0;
                    match self.search_indexes.get(0) {
                        Some(&index) => {
                            self.move_focus_to(index);
                            return Ok(CommandResult::Consumed(None));
                        }
                        None => return Ok(CommandResult::Ignored),
                    }
                }
                JumpMode::Next => {
                    let len = self.search_indexes.len();
                    if len == 0 {
                        return Ok(CommandResult::Ignored);
                    }
                    let index = self.search_selected_index;
                    let next_index = match index.cmp(&(len - 1)) {
                        Ordering::Equal => 0,
                        _ => index + 1,
                    };
                    self.move_focus_to(self.search_indexes[next_index]);
                    self.search_selected_index = next_index;
                    return Ok(CommandResult::Consumed(None));
                }
                JumpMode::Previous => {
                    let len = self.search_indexes.len();
                    if len == 0 {
                        return Ok(CommandResult::Ignored);
                    }
                    let index = self.search_selected_index;
                    let prev_index = match index.cmp(&0) {
                        Ordering::Equal => len - 1,
                        _ => index - 1,
                    };
                    self.move_focus_to(self.search_indexes[prev_index]);
                    self.search_selected_index = prev_index;
                    return Ok(CommandResult::Consumed(None));
                }
            },
            Command::Move(mode, amount) => {
                let last_idx = self.content.read().unwrap().len().saturating_sub(1);

                match mode {
                    MoveMode::Up if self.selected > 0 => {
                        match amount {
                            MoveAmount::Extreme => self.move_focus_to(0),
                            MoveAmount::Integer(amount) => self.move_focus(-(*amount)),
                        }
                        return Ok(CommandResult::Consumed(None));
                    }
                    MoveMode::Down if self.selected < last_idx => {
                        match amount {
                            MoveAmount::Extreme => self.move_focus_to(last_idx),
                            MoveAmount::Integer(amount) => self.move_focus(*amount),
                        }
                        return Ok(CommandResult::Consumed(None));
                    }
                    MoveMode::Down if self.selected == last_idx && self.can_paginate() => {
                        self.pagination.call(&self.content);
                    }
                    _ => {}
                }
            }
            Command::Open(mode) => {
                let queue = self.queue.clone();
                let library = self.library.clone();
                let target: Option<Box<dyn ListItem>> = match mode {
                    TargetMode::Current => self.queue.get_current().map(|t| t.as_listitem()),
                    TargetMode::Selected => {
                        let content = self.content.read().unwrap();
                        content.get(self.selected).map(|t| t.as_listitem())
                    }
                };

                // if item has a dedicated view, show it; otherwise open the context menu
                if let Some(target) = target {
                    let view = target.open(queue.clone(), library.clone());
                    return match view {
                        Some(view) => Ok(CommandResult::View(view)),
                        None => {
                            let contextmenu = ContextMenu::new(&*target, queue, library);
                            Ok(CommandResult::Modal(Box::new(contextmenu)))
                        }
                    };
                }
            }
            Command::Goto(mode) => {
                let mut content = self.content.write().unwrap();
                if let Some(item) = content.get_mut(self.selected) {
                    let queue = self.queue.clone();
                    let library = self.library.clone();

                    match mode {
                        GotoMode::Album => {
                            if let Some(album) = item.album(queue.clone()) {
                                let view =
                                    AlbumView::new(queue, library, &album).as_boxed_view_ext();
                                return Ok(CommandResult::View(view));
                            }
                        }
                        GotoMode::Artist => {
                            if let Some(artist) = item.artist() {
                                let view =
                                    ArtistView::new(queue, library, &artist).as_boxed_view_ext();
                                return Ok(CommandResult::View(view));
                            }
                        }
                    }
                }
            }
            Command::Insert(url) => {
                let url = match url.as_ref().map(String::as_str) {
                    #[cfg(feature = "share_clipboard")]
                    Some("") | None => ClipboardProvider::new()
                        .and_then(|mut ctx: ClipboardContext| ctx.get_contents())
                        .ok()
                        .unwrap(),
                    Some(url) => url.to_owned(),
                    // do nothing if clipboard feature is disabled and there is no url provided
                    #[allow(unreachable_patterns)]
                    _ => return Ok(CommandResult::Consumed(None)),
                };

                let spotify = self.queue.get_spotify();

                let re =
                    Regex::new(r"https?://open\.spotify\.com/(user/[^/]+/)?(\S+)/(\S+)(\?si=\S+)?").unwrap();
                let captures = re.captures(&url);

                if let Some(captures) = captures {
                    let target: Option<Box<dyn ListItem>> = match &captures[2] {
                        "track" => spotify
                            .track(&captures[3])
                            .map(|track| Track::from(&track).as_listitem()),
                        "album" => spotify
                            .album(&captures[3])
                            .map(|album| Album::from(&album).as_listitem()),
                        "playlist" => spotify
                            .playlist(&captures[3])
                            .map(|playlist| Playlist::from(&playlist).as_listitem()),
                        "artist" => spotify
                            .artist(&captures[3])
                            .map(|artist| Artist::from(&artist).as_listitem()),
                        "episode" => spotify
                            .episode(&captures[3])
                            .map(|episode| Episode::from(&episode).as_listitem()),
                        "show" => spotify
                            .get_show(&captures[3])
                            .map(|show| Show::from(&show).as_listitem()),
                        _ => None,
                    };

                    let queue = self.queue.clone();
                    let library = self.library.clone();
                    // if item has a dedicated view, show it; otherwise open the context menu
                    if let Some(target) = target {
                        let view = target.open(queue.clone(), library.clone());
                        return match view {
                            Some(view) => Ok(CommandResult::View(view)),
                            None => {
                                let contextmenu = ContextMenu::new(target.as_ref(), queue, library);
                                Ok(CommandResult::Modal(Box::new(contextmenu)))
                            }
                        };
                    }
                }

                return Ok(CommandResult::Consumed(None));
            }
            _ => {}
        };

        Ok(CommandResult::Ignored)
    }
}
