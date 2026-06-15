use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

use std::sync::{Arc, RwLock};

use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::{Cursive, Printer, Vec2, View};
use ioctl_rs::{TIOCGWINSZ, ioctl};
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
    desired_cover: RwLock<Option<CoverRequest>>,
    rendered_cover: RwLock<Option<CoverRequest>>,
    cover_max_scale: Option<f32>,
    font_size: Vec2,
}

#[derive(Clone, PartialEq, Eq)]
struct CoverRequest {
    url: String,
    path: PathBuf,
    offset: Vec2,
    size: Vec2,
}

impl CoverView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, config: &Config) -> Self {
        // Determine size of window both in pixels and chars
        let (rows, cols, xpixels, ypixels) = unsafe {
            let mut query: (u16, u16, u16, u16) = (0, 0, 0, 0);
            ioctl(1, TIOCGWINSZ, &mut query);
            query
        };

        debug!("Determined window dimensions: {xpixels}x{ypixels}, {cols}x{rows}");

        // Determine font size. Some terminals report physical pixels here, but
        // the aspect ratio is still useful when mapping images to terminal cells.
        let font_size = if cols == 0 || rows == 0 || xpixels == 0 || ypixels == 0 {
            Vec2::new(8, 16)
        } else {
            Vec2::new(
                std::cmp::max(1, xpixels / cols) as usize,
                std::cmp::max(1, ypixels / rows) as usize,
            )
        };

        debug!("Determined font size: {}x{}", font_size.x, font_size.y);

        Self {
            queue,
            library,
            loading: Arc::new(RwLock::new(HashSet::new())),
            desired_cover: RwLock::new(None),
            rendered_cover: RwLock::new(None),
            cover_max_scale: config.values().cover_max_scale,
            font_size,
        }
    }

    fn draw_cover(&self, url: String, mut draw_offset: Vec2, draw_size: Vec2) {
        if draw_size.x <= 1 || draw_size.y <= 1 {
            return;
        }

        let path = match self.cache_path(url.clone()) {
            Some(p) => p,
            None => return,
        };

        let image_size = image::image_dimensions(&path).unwrap_or((640, 640));
        let mut size = self.cover_size(draw_size, image_size);

        // Make sure there is equal space in chars on either side
        if size.x > 1 && size.x % 2 != draw_size.x % 2 {
            size.x -= 1;
        }

        // Make sure x is the bottleneck so full width is used
        size.y = std::cmp::min(draw_size.y, size.y + 1);

        // Round up since the bottom might have empty space within
        // the designated box
        draw_offset.x += (draw_size.x - size.x) / 2;
        draw_offset.y += (draw_size.y - size.y) - (draw_size.y - size.y) / 2;

        let mut desired_cover = self.desired_cover.write().unwrap();
        *desired_cover = Some(CoverRequest {
            url,
            path,
            offset: draw_offset,
            size,
        });
    }

    fn clear_cover(&self) {
        let mut desired_cover = self.desired_cover.write().unwrap();
        *desired_cover = None;
    }

    fn cover_size(&self, draw_size: Vec2, image_size: (u32, u32)) -> Vec2 {
        let (image_width, image_height) = image_size;
        if image_width == 0 || image_height == 0 {
            return draw_size;
        }

        let mut available_size = draw_size;
        if let Some(scale) = self.cover_max_scale {
            let max_size = Vec2::new(
                ((image_width as f32 * scale) / self.font_size.x as f32) as usize,
                ((image_height as f32 * scale) / self.font_size.y as f32) as usize,
            );
            available_size.x = std::cmp::min(available_size.x, std::cmp::max(1, max_size.x));
            available_size.y = std::cmp::min(available_size.y, std::cmp::max(1, max_size.y));
        }

        fit_image_to_cells(available_size, self.font_size, image_width, image_height)
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
                error!("Failed to download cover: {e}");
            }
            let mut loading = loading_thread.write().unwrap();
            loading.remove(&url.clone());
        });

        None
    }

    pub fn render_to_terminal(&self) {
        let desired_cover = self.desired_cover.read().unwrap().clone();
        let mut rendered_cover = self.rendered_cover.write().unwrap();

        if *rendered_cover == desired_cover {
            return;
        }

        if let Some(rendered) = rendered_cover.as_ref() {
            clear_terminal_area(rendered.offset, rendered.size);
        }

        if let Some(cover) = desired_cover.as_ref()
            && let Err(e) = render_cover_to_terminal(cover)
        {
            error!("Failed to draw cover: {e}");
            return;
        }

        *rendered_cover = desired_cover;
    }
}

fn render_cover_to_terminal(cover: &CoverRequest) -> Result<(), viuer::ViuError> {
    let config = viuer::Config {
        x: to_u16(cover.offset.x)?,
        y: to_i16(cover.offset.y)?,
        width: Some(cover.size.x as u32),
        height: Some(cover.size.y as u32),
        absolute_offset: true,
        restore_cursor: true,
        use_kitty: can_use_kitty_graphics(),
        use_sixel: !is_iterm_terminal(),
        ..Default::default()
    };

    let image = image::ImageReader::open(&cover.path)?
        .with_guessed_format()?
        .decode()?;

    viuer::print(&image, &config).map(|_| ())
}

fn is_iterm_terminal() -> bool {
    std::env::var("TERM_PROGRAM").is_ok_and(|term| term.contains("iTerm"))
        || std::env::var("LC_TERMINAL").is_ok_and(|term| term.contains("iTerm"))
}

fn is_apple_terminal() -> bool {
    std::env::var("TERM_PROGRAM").is_ok_and(|term| term == "Apple_Terminal")
}

fn can_use_kitty_graphics() -> bool {
    !is_apple_terminal()
}

fn fit_image_to_cells(
    available_size: Vec2,
    font_size: Vec2,
    image_width: u32,
    image_height: u32,
) -> Vec2 {
    if available_size.x == 0 || available_size.y == 0 || font_size.x == 0 || font_size.y == 0 {
        return Vec2::new(0, 0);
    }

    let image_aspect = image_width as f32 / image_height as f32;
    let cell_aspect = font_size.x as f32 / font_size.y as f32;
    let width_for_full_height =
        (available_size.y as f32 * image_aspect / cell_aspect).floor() as usize;

    if width_for_full_height <= available_size.x {
        Vec2::new(std::cmp::max(1, width_for_full_height), available_size.y)
    } else {
        let height_for_full_width =
            (available_size.x as f32 * cell_aspect / image_aspect).floor() as usize;
        Vec2::new(available_size.x, std::cmp::max(1, height_for_full_width))
    }
}

fn clear_terminal_area(offset: Vec2, size: Vec2) {
    let mut stdout = std::io::stdout();

    // Remove stateful Kitty graphics where that protocol is available, then
    // overwrite the cells used by other protocols/fallbacks.
    if can_use_kitty_graphics() {
        let _ = stdout.write_all(b"\x1b_Ga=d,d=A\x1b\\");
    }
    for y in offset.y..offset.y + size.y {
        let _ = write!(
            stdout,
            "\x1b[{};{}H{}",
            y + 1,
            offset.x + 1,
            " ".repeat(size.x)
        );
    }
    let _ = stdout.flush();
}

fn to_u16(value: usize) -> Result<u16, viuer::ViuError> {
    u16::try_from(value).map_err(|_| {
        viuer::ViuError::InvalidConfiguration("cover coordinate is too large".to_string())
    })
}

fn to_i16(value: usize) -> Result<i16, viuer::ViuError> {
    i16::try_from(value).map_err(|_| {
        viuer::ViuError::InvalidConfiguration("cover coordinate is too large".to_string())
    })
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
        self.render_to_terminal();
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
                    crate::sharing::write_share(url).ok();
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
