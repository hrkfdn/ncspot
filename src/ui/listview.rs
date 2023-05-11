use cursive::view::scroll::Scroller;
use log::info;
use std::cmp::{max, min, Ordering};
use std::sync::{Arc, RwLock};

use cursive::align::HAlign;
use cursive::event::{Callback, Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::view::scroll;
use cursive::{Cursive, Printer, Rect, Vec2};
use unicode_width::UnicodeWidthStr;

use crate::command::{Command, GotoMode, InsertSource, JumpMode, MoveAmount, MoveMode, TargetMode};
use crate::commands::CommandResult;
use crate::ext_traits::CursiveExt;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::queue::Queue;
#[cfg(feature = "share_clipboard")]
use crate::sharing::{read_share, write_share};
use crate::spotify::UriType;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::album::AlbumView;
use crate::ui::artist::ArtistView;
use crate::ui::contextmenu::ContextMenu;
use crate::ui::pagination::Pagination;

pub struct ListView<I: ListItem> {
    content: Arc<RwLock<Vec<I>>>,
    last_content_len: usize,
    selected: usize,
    search_query: String,
    search_indexes: Vec<usize>,
    search_selected_index: usize,
    last_size: Vec2,
    scroller: scroll::Core,
    queue: Arc<Queue>,
    library: Arc<Library>,
    pagination: Pagination<I>,
    title: String,
}

impl<I: ListItem> Scroller for ListView<I> {
    fn get_scroller_mut(&mut self) -> &mut scroll::Core {
        &mut self.scroller
    }

    fn get_scroller(&self) -> &scroll::Core {
        &self.scroller
    }
}

impl<I: ListItem + Clone> ListView<I> {
    pub fn new(content: Arc<RwLock<Vec<I>>>, queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let result = Self {
            content,
            last_content_len: 0,
            selected: 0,
            search_query: String::new(),
            search_indexes: Vec::new(),
            search_selected_index: 0,
            last_size: Vec2::new(0, 0),
            scroller: scroll::Core::new(),
            queue,
            library,
            pagination: Pagination::default(),
            title: "".to_string(),
        };
        result.try_paginate();
        result
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn get_pagination(&self) -> &Pagination<I> {
        &self.pagination
    }

    /// Return the current amount of items in `content`
    ///
    /// If `include_paginator` is `true`, the pagination entry will be included
    /// in the count.
    pub fn content_len(&self, include_paginator: bool) -> usize {
        let content_len = self.content.read().unwrap().len();

        // add 1 more row for paginator if we can paginate
        if self.can_paginate() && include_paginator {
            content_len + 1
        } else {
            content_len
        }
    }

    /// Return wether there are still items that aren't shown in the listview.
    ///
    /// `true` if there are unloaded items
    /// `false` if all items are loaded
    fn can_paginate(&self) -> bool {
        self.get_pagination().max_content().unwrap_or(0) > self.get_pagination().loaded_content()
    }

    /// Try to load more items into the list if neccessary.
    #[inline]
    fn try_paginate(&self) {
        // Paginate if there are more items
        //  AND
        //   The selected item is the current last item (keyboard scrolling)
        //    OR
        //   The scroller can't scroll further down (mouse scrolling)
        if self.can_paginate()
            && (self.selected == self.content.read().unwrap().len().saturating_sub(1)
                || !self.scroller.can_scroll_down())
        {
            self.pagination.call(&self.content, self.library.clone());
        }
    }

    /// Return whether this [ListView] has visible scrollbars.
    pub fn has_visible_scrollbars(&self) -> bool {
        self.scroller.get_show_scrollbars()
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
                i.display_left(&self.library)
                    .to_lowercase()
                    .contains(&query[..].to_lowercase())
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn move_focus_to(&mut self, target: usize) {
        let len = self.content_len(false).saturating_sub(1);
        self.selected = min(target, len);
        self.scroller.scroll_to_y(self.selected);
    }

    pub fn move_focus(&mut self, delta: i32) {
        let new = self.selected as i32 + delta;
        self.move_focus_to(max(new, 0) as usize);
    }

    /// Append the currently selected item and all the following ones to the queue after the
    /// currently playing track and start playing them. Returns true if adding and playing the
    /// tracks succeeded, false otherwhise.
    fn attempt_play_all_tracks(&self) -> bool {
        let content = self.content.read().unwrap();
        let any = &(*content) as &dyn std::any::Any;
        let playables = any.downcast_ref::<Vec<Playable>>();
        let tracks = any.downcast_ref::<Vec<Track>>().map(|t| {
            t.iter()
                .map(|t| Playable::Track(t.clone()))
                .collect::<Vec<Playable>>()
        });
        if let Some(tracks) = playables.or(tracks.as_ref()) {
            let index = self.queue.append_next(tracks);
            self.queue.play(index + self.selected, true, false);
            true
        } else {
            false
        }
    }

    /// Appends the currently focused item after the currently playing item and starts playing it.
    fn play_current_item(&mut self) {
        let mut content = self.content.write().unwrap();
        if let Some(listitem) = content.get_mut(self.selected) {
            listitem.play(&self.queue);
        }
    }

    /// Remove the item at `index` from the list.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn remove(&mut self, index: usize) {
        let mut c = self.content.write().unwrap();
        c.remove(index);
        if self.selected >= c.len() {
            self.selected = self.selected.saturating_sub(1);
        }
    }
}

impl<I: ListItem + Clone> View for ListView<I> {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let content = self.content.read().unwrap();

        scroll::draw_lines(self, printer, |_, printer, i| {
            // draw paginator after content
            if i == content.len() && self.can_paginate() {
                let style = ColorStyle::secondary();

                let max = self.pagination.max_content().unwrap();
                let buf = format!("{} more items, scroll to load", max - i);
                printer.with_color(style, |printer| {
                    printer.print((0, 0), &buf);
                });
            } else if i < content.len() {
                let item = &content[i];
                let currently_playing =
                    item.is_playing(&self.queue) && self.queue.get_current_index() == Some(i);

                let style = if self.selected == i {
                    if currently_playing {
                        ColorStyle::new(
                            *printer.theme.palette.custom("playing_selected").unwrap(),
                            ColorType::Palette(PaletteColor::Highlight),
                        )
                    } else {
                        ColorStyle::highlight()
                    }
                } else if currently_playing {
                    ColorStyle::new(
                        ColorType::Color(*printer.theme.palette.custom("playing").unwrap()),
                        ColorType::Color(*printer.theme.palette.custom("playing_bg").unwrap()),
                    )
                } else {
                    ColorStyle::primary()
                };

                let left = item.display_left(&self.library);
                let center = item.display_center(&self.library);
                let right = item.display_right(&self.library);
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
                            printer.print((left[0..m.0].width(), 0), &left[m.0..m.1]);
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
        self.last_size = size;

        let relayout_scroller = self.content_len(false) != self.last_content_len;
        self.last_content_len = self.content_len(true);

        scroll::layout(
            self,
            size,
            relayout_scroller,
            |_, _| {},
            |s, c| Vec2::new(c.x, s.content_len(true)),
        );
    }

    fn needs_relayout(&self) -> bool {
        self.scroller.needs_relayout()
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn on_event(&mut self, e: Event) -> EventResult {
        match e {
            Event::Mouse {
                event: MouseEvent::WheelUp,
                ..
            } => self.scroller.scroll_up(3),
            Event::Mouse {
                event: MouseEvent::WheelDown,
                ..
            } => {
                self.scroller.scroll_down(3);
                self.try_paginate();
            }
            Event::Mouse {
                event: MouseEvent::Press(MouseButton::Left),
                position,
                offset,
            } => {
                // This is safe as a mouse event is only propagated to a view when it is inside the
                // view. Therefore underflow shouldn't occur.
                let view_coordinates_click_position = position - offset;

                let drag_started = if self.has_visible_scrollbars() {
                    self.scroller.start_drag(view_coordinates_click_position)
                } else {
                    false
                };

                if drag_started {
                    log::debug!("grabbing scroller");
                } else {
                    let viewport = self.scroller.content_viewport().top_left();
                    let selected_row = position.checked_sub(offset).map(|p| p.y + viewport.y);
                    if let Some(clicked_row_index) =
                        selected_row.filter(|row| *row < self.content_len(false))
                    {
                        let currently_selected_listitem = self
                            .content
                            .read()
                            .unwrap()
                            .get(clicked_row_index)
                            .map(ListItem::as_listitem);
                        let currently_selected_is_individual = currently_selected_listitem
                            .filter(|item| item.track().is_some())
                            .is_some();
                        if self.selected == clicked_row_index && currently_selected_is_individual {
                            // The selected position was already focused. Play the item at the
                            // position as if Enter was pressed. This sort of emulates double
                            // clicking, which isn't supported by Cursive.
                            self.queue.clear();

                            if !self.attempt_play_all_tracks() {
                                self.play_current_item();
                            }
                        } else {
                            // The clicked position wasn't focused yet or the item is a collection
                            // that can be opened.
                            self.move_focus_to(clicked_row_index);
                            let content = self.content.read().unwrap();
                            let clicked_list_item =
                                content.get(self.selected).map(ListItem::as_listitem);

                            if let Some(target) = clicked_list_item {
                                if let Some(view) =
                                    target.open(self.queue.clone(), self.library.clone())
                                {
                                    return EventResult::Consumed(Some(Callback::from_fn_once(
                                        move |s| {
                                            s.on_layout(|_, mut l| l.push_view(view));
                                        },
                                    )));
                                }
                            }
                        }
                    }
                }
            }
            Event::Mouse {
                event: MouseEvent::Press(MouseButton::Right),
                position,
                offset,
            } => {
                let viewport = self.scroller.content_viewport().top_left();
                let selected_row = position.checked_sub(offset).map(|p| p.y + viewport.y);
                if let Some(y) = selected_row.filter(|row| row < &self.content_len(false)) {
                    self.move_focus_to(y);

                    let queue = self.queue.clone();
                    let library = self.library.clone();
                    if let Some(target) = {
                        let content = self.content.read().unwrap();
                        content.get(self.selected).map(|t| t.as_listitem())
                    } {
                        let contextmenu = ContextMenu::new(&*target, queue, library);
                        return EventResult::Consumed(Some(Callback::from_fn_once(move |s| {
                            s.add_layer(contextmenu)
                        })));
                    }
                }
            }
            Event::Mouse {
                event: MouseEvent::Hold(MouseButton::Left),
                position,
                offset,
            } => {
                if self.has_visible_scrollbars() {
                    self.scroller.drag(position.saturating_sub(offset));
                }
            }
            Event::Mouse {
                event: MouseEvent::Release(MouseButton::Left),
                ..
            } => {
                log::debug!("releasing scroller");
                self.scroller.release_grab();
            }
            _ => {
                return EventResult::Ignored;
            }
        }

        EventResult::Consumed(None)
    }

    fn important_area(&self, view_size: Vec2) -> Rect {
        if self.content_len(false) > 0 {
            Rect::from_point((view_size.x, self.selected))
        } else {
            Rect::from_point((0, 0))
        }
    }
}

impl<I: ListItem + Clone> ViewExt for ListView<I> {
    fn title(&self) -> String {
        self.title.clone()
    }

    fn on_command(&mut self, _s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Play => {
                self.queue.clear();

                if !self.attempt_play_all_tracks() {
                    self.play_current_item();
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::PlayNext => {
                info!("played next");
                let mut content = self.content.write().unwrap();
                if let Some(item) = content.get_mut(self.selected) {
                    item.play_next(&self.queue);
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Queue => {
                let mut content = self.content.write().unwrap();
                if let Some(item) = content.get_mut(self.selected) {
                    item.queue(&self.queue);
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Save => {
                let mut item = {
                    let content = self.content.read().unwrap();
                    content.get(self.selected).cloned()
                };

                if let Some(item) = item.as_mut() {
                    item.save(&self.library);
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Delete => {
                let mut item = {
                    let content = self.content.read().unwrap();
                    content.get(self.selected).cloned()
                };

                if let Some(item) = item.as_mut() {
                    item.unsave(&self.library);
                }

                return Ok(CommandResult::Consumed(None));
            }
            #[cfg(feature = "share_clipboard")]
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
                    write_share(url);
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Jump(mode) => match mode {
                JumpMode::Query(query) => {
                    self.search_query = query.to_lowercase();
                    self.search_indexes = self.get_indexes_of(query);
                    self.search_selected_index = 0;
                    match self.search_indexes.first() {
                        Some(&index) => {
                            self.move_focus_to(index);
                            return Ok(CommandResult::Consumed(None));
                        }
                        None => return Ok(CommandResult::Consumed(None)),
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
                    MoveMode::Up => {
                        if self.selected > 0 {
                            match amount {
                                MoveAmount::Extreme => self.move_focus_to(0),
                                MoveAmount::Float(scale) => {
                                    let amount = (self.last_size.y as f32) * scale;
                                    self.move_focus(-(amount as i32))
                                }
                                MoveAmount::Integer(amount) => self.move_focus(-(*amount)),
                            }
                        }
                        return Ok(CommandResult::Consumed(None));
                    }
                    MoveMode::Down => {
                        if self.selected < last_idx {
                            match amount {
                                MoveAmount::Extreme => self.move_focus_to(last_idx),
                                MoveAmount::Float(scale) => {
                                    let amount = (self.last_size.y as f32) * scale;
                                    self.move_focus(amount as i32)
                                }
                                MoveAmount::Integer(amount) => self.move_focus(*amount),
                            }
                        }
                        self.try_paginate();
                        return Ok(CommandResult::Consumed(None));
                    }
                    _ => return Ok(CommandResult::Consumed(None)),
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
                            if let Some(album) = item.album(&queue) {
                                let view =
                                    AlbumView::new(queue, library, &album).into_boxed_view_ext();
                                return Ok(CommandResult::View(view));
                            }
                        }
                        GotoMode::Artist => {
                            if let Some(artists) = item.artists() {
                                return match artists.len() {
                                    0 => Ok(CommandResult::Consumed(None)),
                                    1 => {
                                        let view = ArtistView::new(queue, library, &artists[0])
                                            .into_boxed_view_ext();
                                        Ok(CommandResult::View(view))
                                    }
                                    _ => {
                                        let dialog = ContextMenu::select_artist_dialog(
                                            library, queue, artists,
                                        );
                                        Ok(CommandResult::Modal(Box::new(dialog)))
                                    }
                                };
                            }
                        }
                    }
                }
            }
            Command::Insert(source) => {
                let url = match source {
                    InsertSource::Input(url) => Some(url.clone()),
                    #[cfg(feature = "share_clipboard")]
                    InsertSource::Clipboard => {
                        read_share().and_then(crate::spotify_url::SpotifyUrl::from_url)
                    }
                };

                let spotify = self.queue.get_spotify();

                if let Some(url) = url {
                    let target: Option<Box<dyn ListItem>> = match url.uri_type {
                        UriType::Track => spotify
                            .api
                            .track(&url.id)
                            .map(|track| Track::from(&track).as_listitem()),
                        UriType::Album => spotify
                            .api
                            .album(&url.id)
                            .map(|album| Album::from(&album).as_listitem()),
                        UriType::Playlist => spotify
                            .api
                            .playlist(&url.id)
                            .map(|playlist| Playlist::from(&playlist).as_listitem()),
                        UriType::Artist => spotify
                            .api
                            .artist(&url.id)
                            .map(|artist| Artist::from(&artist).as_listitem()),
                        UriType::Episode => spotify
                            .api
                            .episode(&url.id)
                            .map(|episode| Episode::from(&episode).as_listitem()),
                        UriType::Show => spotify
                            .api
                            .get_show(&url.id)
                            .map(|show| Show::from(&show).as_listitem()),
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
            Command::ShowRecommendations(mode) => {
                let queue = self.queue.clone();
                let library = self.library.clone();
                let target: Option<Box<dyn ListItem>> = match mode {
                    TargetMode::Current => self.queue.get_current().map(|t| t.as_listitem()),
                    TargetMode::Selected => {
                        let content = self.content.read().unwrap();
                        content.get(self.selected).map(|t| t.as_listitem())
                    }
                };

                if let Some(mut target) = target {
                    let view = target.open_recommendations(queue, library);
                    return match view {
                        Some(view) => Ok(CommandResult::View(view)),
                        None => Ok(CommandResult::Consumed(None)),
                    };
                }
            }
            _ => {}
        };

        Ok(CommandResult::Ignored)
    }
}
