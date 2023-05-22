use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Stdio};

use std::sync::{Arc, RwLock};

use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::{Cursive, Printer, Vec2, View};
use ioctl_rs::{ioctl, TIOCGWINSZ};
use log::{debug, error};

use crate::command::{Command, GotoMode};
use crate::commands::CommandResult;
use crate::config::Config;
use crate::library::Library;
use crate::queue::Queue;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::album::AlbumView;
use crate::ui::artist::ArtistView;

pub struct CoverView {
    queue: Arc<Queue>,
    library: Arc<Library>,
    loading: Arc<RwLock<HashSet<String>>>,
    last_size: RwLock<Vec2>,
    drawn_url: RwLock<Option<String>>,
    ueberzug: RwLock<Option<Child>>,
    font_size: Vec2,
}

impl CoverView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, config: &Config) -> Self {
        // Determine size of window both in pixels and chars
        let (rows, cols, mut xpixels, mut ypixels) = unsafe {
            let query: (u16, u16, u16, u16) = (0, 0, 0, 0);
            ioctl(1, TIOCGWINSZ, &query);
            query
        };

        debug!(
            "Determined window dimensions: {}x{}, {}x{}",
            xpixels, ypixels, cols, rows
        );

        // Determine font size, considering max scale to prevent tiny covers on HiDPI screens
        let scale = config.values().cover_max_scale.unwrap_or(1.0);
        xpixels = ((xpixels as f32) / scale) as u16;
        ypixels = ((ypixels as f32) / scale) as u16;

        let font_size = Vec2::new((xpixels / cols) as usize, (ypixels / rows) as usize);

        debug!("Determined font size: {}x{}", font_size.x, font_size.y);

        Self {
            queue,
            library,
            ueberzug: RwLock::new(None),
            loading: Arc::new(RwLock::new(HashSet::new())),
            last_size: RwLock::new(Vec2::new(0, 0)),
            drawn_url: RwLock::new(None),
            font_size,
        }
    }

    fn draw_cover(&self, url: String, mut draw_offset: Vec2, draw_size: Vec2) {
        if draw_size.x <= 1 || draw_size.y <= 1 {
            return;
        }

        let needs_redraw = {
            let last_size = self.last_size.read().unwrap();
            let drawn_url = self.drawn_url.read().unwrap();
            *last_size != draw_size || drawn_url.as_ref() != Some(&url)
        };

        if !needs_redraw {
            return;
        }

        let path = match self.cache_path(url.clone()) {
            Some(p) => p,
            None => return,
        };

        let mut img_size = Vec2::new(640, 640);

        let draw_size_pxls = draw_size * self.font_size;
        let ratio = f32::min(
            f32::min(
                draw_size_pxls.x as f32 / img_size.x as f32,
                draw_size_pxls.y as f32 / img_size.y as f32,
            ),
            1.0,
        );

        img_size = Vec2::new(
            (ratio * img_size.x as f32) as usize,
            (ratio * img_size.y as f32) as usize,
        );

        // Ueberzug takes an area given in chars and fits the image to
        // that area (from the top left). Since we want to center the
        // image at least horizontally, we need to fiddle around a bit.
        let mut size = img_size / self.font_size;

        // Make sure there is equal space in chars on either side
        if size.x % 2 != draw_size.x % 2 {
            size.x -= 1;
        }

        // Make sure x is the bottleneck so full width is used
        size.y = std::cmp::min(draw_size.y, size.y + 1);

        // Round up since the bottom might have empty space within
        // the designated box
        draw_offset.x += (draw_size.x - size.x) / 2;
        draw_offset.y += (draw_size.y - size.y) - (draw_size.y - size.y) / 2;

        let cmd = format!("{{\"action\":\"add\",\"scaler\":\"fit_contain\",\"identifier\":\"cover\",\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"path\":\"{}\"}}\n",
            draw_offset.x, draw_offset.y,
            size.x, size.y,
            path.to_str().unwrap()
        );

        if let Err(e) = self.run_ueberzug_cmd(&cmd) {
            error!("Failed to run Ueberzug: {}", e);
            return;
        }

        let mut last_size = self.last_size.write().unwrap();
        *last_size = draw_size;

        let mut drawn_url = self.drawn_url.write().unwrap();
        *drawn_url = Some(url);
    }

    fn clear_cover(&self) {
        let mut drawn_url = self.drawn_url.write().unwrap();
        *drawn_url = None;

        let cmd = "{\"action\": \"remove\", \"identifier\": \"cover\"}\n";
        if let Err(e) = self.run_ueberzug_cmd(cmd) {
            error!("Failed to run Ueberzug: {}", e);
        }
    }

    fn run_ueberzug_cmd(&self, cmd: &str) -> Result<(), std::io::Error> {
        let mut ueberzug = self.ueberzug.write().unwrap();

        if ueberzug.is_none() {
            *ueberzug = Some(
                std::process::Command::new("ueberzug")
                    .args(["layer", "--silent"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?,
            );
        }

        let stdin = (*ueberzug).as_mut().unwrap().stdin.as_mut().unwrap();
        stdin.write_all(cmd.as_bytes())?;

        Ok(())
    }

    fn cache_path(&self, url: String) -> Option<PathBuf> {
        let path = crate::utils::cache_path_for_url(url.clone());

        let mut loading = self.loading.write().unwrap();
        if loading.contains(&url) {
            return None;
        }

        if path.exists() {
            return Some(path);
        }

        loading.insert(url.clone());

        let loading_thread = self.loading.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::utils::download(url.clone(), path.clone()) {
                error!("Failed to download cover: {}", e);
            }
            let mut loading = loading_thread.write().unwrap();
            loading.remove(&url.clone());
        });

        None
    }
}

impl View for CoverView {
    fn draw(&self, printer: &Printer<'_, '_>) {
        // Completely blank out screen
        let style = ColorStyle::new(
            ColorType::Palette(PaletteColor::Background),
            ColorType::Palette(PaletteColor::Background),
        );
        printer.with_color(style, |printer| {
            for i in 0..printer.size.y {
                printer.print_hline((0, i), printer.size.x, " ");
            }
        });

        let cover_url = self.queue.get_current().and_then(|t| t.cover_url());

        if let Some(url) = cover_url {
            self.draw_cover(url, printer.offset, printer.size);
        } else {
            self.clear_cover();
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, 2)
    }
}

impl ViewExt for CoverView {
    fn title(&self) -> String {
        "Cover".to_string()
    }

    fn on_leave(&self) {
        self.clear_cover();
    }

    fn on_command(&mut self, _s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Save => {
                if let Some(mut track) = self.queue.get_current() {
                    track.save(&self.library);
                }
            }
            Command::Delete => {
                if let Some(mut track) = self.queue.get_current() {
                    track.unsave(&self.library);
                }
            }
            #[cfg(feature = "share_clipboard")]
            Command::Share(_mode) => {
                let url = self
                    .queue
                    .get_current()
                    .and_then(|t| t.as_listitem().share_url());

                if let Some(url) = url {
                    crate::sharing::write_share(url);
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Goto(mode) => {
                if let Some(track) = self.queue.get_current() {
                    let queue = self.queue.clone();
                    let library = self.library.clone();

                    match mode {
                        GotoMode::Album => {
                            if let Some(album) = track.album(&queue) {
                                let view =
                                    AlbumView::new(queue, library, &album).into_boxed_view_ext();
                                return Ok(CommandResult::View(view));
                            }
                        }
                        GotoMode::Artist => {
                            if let Some(artists) = track.artists() {
                                return match artists.len() {
                                    0 => Ok(CommandResult::Consumed(None)),
                                    // Always choose the first artist even with more because
                                    // the cover image really doesn't play nice with the menu
                                    _ => {
                                        let view = ArtistView::new(queue, library, &artists[0])
                                            .into_boxed_view_ext();
                                        Ok(CommandResult::View(view))
                                    }
                                };
                            }
                        }
                    }
                }
            }
            _ => {}
        };

        Ok(CommandResult::Ignored)
    }
}
