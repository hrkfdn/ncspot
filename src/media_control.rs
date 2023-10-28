use std::time::Duration;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use souvlaki::{
    MediaControlEvent, 
    MediaControls, 
    MediaMetadata, 
    MediaPlayback, 
    MediaPosition, 
    PlatformConfig, 
    SeekDirection,
};

use crate::application::ASYNC_RUNTIME;
use crate::queue::Queue;
use crate::model::playable::Playable;
use crate::spotify::{
    PlayerEvent,
    Spotify,
};
use crate::traits::ListItem;

struct MediaControlPlayer {
    spotify: Spotify,
    queue: Arc<Queue>,
    controls: MediaControls,
    last_track: Option<Playable>,
}

enum MediaControlPlayerEvent {
    SouvlakiEvent(MediaControlEvent),
    UpdateEvent,
}

impl MediaControlPlayer {
    fn handle(&mut self, e: MediaControlPlayerEvent) {
        match e {
            MediaControlPlayerEvent::UpdateEvent => {
                self.update();
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Play) => {
                self.spotify.play();
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Pause) => {
                self.spotify.pause();
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Toggle) => {
                self.queue.toggleplayback();
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Next) => {
                self.queue.next(true);
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Previous) => {
                if self.spotify.get_current_progress() < Duration::from_secs(5) {
                    self.queue.previous();
                } else {
                    self.spotify.seek(0);
                }
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Stop) => {
                self.queue.stop();
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Seek(SeekDirection::Forward)) => {
                self.seek(5i32)
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Seek(SeekDirection::Backward)) => {
                self.seek(-5i32)
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::SeekBy(SeekDirection::Forward, dur)) => {
                self.seek(dur.as_secs() as i32)
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::SeekBy(SeekDirection::Backward, dur)) => {
                self.seek(-(dur.as_secs() as i32));
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::SetPosition(MediaPosition(dur))) => {
                if let Some(current_track) = self.queue.get_current() {
                    let position = dur.as_secs() as u32;
                    let duration = current_track.duration();
        
                    if position < duration {
                        self.spotify.seek(position);
                    }
                }
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::OpenUri(uri)) => {
                self.queue.open_uri(uri.as_str());
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Raise) => {
            }
            MediaControlPlayerEvent::SouvlakiEvent(MediaControlEvent::Quit) => {
            }
        }
    }

    fn update(&mut self) {
        let playable = self.queue.get_current();

        if self.last_track != playable {
            let playable = self.queue.get_current().and_then(|p| match p {
                Playable::Track(track) => {
                    if track.cover_url.is_some() {
                        // We already have `cover_url`, no need to fetch the full track
                        Some(Playable::Track(track))
                    } else {
                        self.spotify
                            .api
                            .track(&track.id.unwrap_or_default())
                            .as_ref()
                            .map(|t| Playable::Track(t.into()))
                    }
                }
                Playable::Episode(episode) => Some(Playable::Episode(episode)),
            });

            let _ = self.controls.set_metadata(MediaMetadata {
                title: playable
                    .as_ref()
                    .map(|t| match t {
                        Playable::Track(t) => t.title.as_str(),
                        Playable::Episode(ep) => ep.name.as_str(),
                    }),
                album: playable
                    .as_ref()
                    .and_then(|p| p.track())
                    .and_then(|t| t.album)
                    .as_deref(),
                artist: playable
                    .as_ref()
                    .and_then(|p| p.track())
                    .and_then(|t| t.album_artists.into_iter().nth(0)) // just the first artist
                    .as_deref(),
                duration: playable
                    .as_ref()
                    .map(|t| Duration::from_secs(t.duration() as u64)),
                cover_url: playable
                    .as_ref()
                    .and_then(|t| t.cover_url())
                    .as_deref(),
            });
        }

        let _ = self.controls.set_playback(
            match self.spotify.get_current_status() {
                PlayerEvent::FinishedTrack => MediaPlayback::Playing { progress: None },
                PlayerEvent::Paused(dur) => MediaPlayback::Paused { progress: Some(MediaPosition(dur)) },
                PlayerEvent::Playing(_) => MediaPlayback::Playing { progress: Some(MediaPosition(self.spotify.get_current_progress())) },
                PlayerEvent::Stopped => MediaPlayback::Stopped,
            }
        );
    }

    fn seek(&self, secs_delta: i32) {
        if let Some(current_track) = self.queue.get_current() {
            let progress = self.spotify.get_current_progress();
            let new_position = (progress.as_millis() as i32 + (secs_delta * 1000)) as i32;

            if new_position <= 0 {
                self.queue.previous();
            }
            else if new_position < current_track.duration() as i32 {
                self.spotify.seek(new_position as u32);
            } else {
                self.queue.next(true);
            }
        }
    }
}

pub struct MediaControlManager {
    tx: mpsc::UnboundedSender<MediaControlPlayerEvent>,
    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    window: crate::media_control::windows::DummyWindow, // preventing window from getting dropped too early
}

impl MediaControlManager {
    pub fn new(
        spotify: Spotify,
        queue: Arc<Queue>,
    ) -> Result<Self, souvlaki::Error> {
        
        #[cfg(not(target_os = "windows"))]
        let hwnd = None;

        #[cfg(target_os = "windows")]
        let (hwnd, window) = {
            let window = windows::DummyWindow::new().unwrap();
            (Some(window.handle.0 as _), window)
        };

        let (tx, rx) = mpsc::unbounded_channel::<MediaControlPlayerEvent>();
        let txc = tx.clone();

        let mut controls = MediaControls::new(PlatformConfig {
            dbus_name: instance_bus_name().as_str(),
            display_name: "ncspot",
            hwnd,
        })?;
        
        controls.attach(move |e| { // TODO: probably better to use second channel for second message type
            if let Err(e) = txc.send(MediaControlPlayerEvent::SouvlakiEvent(e)) {
                log::warn!("Could not process Media Control event: {e}");
            }
        })?;

        let player = MediaControlPlayer {
            controls: controls,
            last_track: None,
            queue: queue,
            spotify: spotify,
        };
        
        ASYNC_RUNTIME.get().unwrap().spawn(async {
            let result = Self::serve(UnboundedReceiverStream::new(rx), player).await;
            if let Err(e) = result {
                log::error!("Media Control error: {e}");
            }
        });

        Ok(Self { 
            tx,
            #[cfg(target_os = "windows")]
            window,
        })
    }

    async fn serve(
        mut rx: UnboundedReceiverStream<MediaControlPlayerEvent>,
        mut player: MediaControlPlayer
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        loop {
            if let Some(e) = rx.next().await {
                player.handle(e);
                
                #[cfg(target_os = "windows")]
                windows::pump_event_queue();
            }
        }
    }

    pub fn update(&self) {
        if let Err(e) = self.tx.send(MediaControlPlayerEvent::UpdateEvent) {
            log::warn!("Could not update Media Control state: {e}");
        }
    }
}

/// Get the D-Bus bus name for this instance according to the MPRIS specification.
///
/// https://specifications.freedesktop.org/mpris-spec/2.2/#Bus-Name-Policy
pub fn instance_bus_name() -> String {
    format!(
        "org.mpris.MediaPlayer2.ncspot.instance{}",
        std::process::id()
    )
}

// demonstrates how to make a minimal window to allow use of media keys on the command line
// ref: https://github.com/Sinono3/souvlaki/blob/master/examples/print_events.rs
#[cfg(target_os = "windows")]
mod windows {
    use std::io::Error;
    use std::mem;

    use windows::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor,
        IsDialogMessageW, PeekMessageW, RegisterClassExW, TranslateMessage, GA_ROOT, MSG,
        PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_QUIT, WNDCLASSEXW,
    };

    pub struct DummyWindow {
        pub handle: HWND,
    }

    impl DummyWindow {
        pub fn new() -> Result<DummyWindow, String> {
            let class_name = w!("SimpleTray");

            let handle_result = unsafe {
                let instance = GetModuleHandleW(None)
                    .map_err(|e| (format!("Getting module handle failed: {e}")))?;

                let wnd_class = WNDCLASSEXW {
                    cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
                    hInstance: instance,
                    lpszClassName: class_name,
                    lpfnWndProc: Some(Self::wnd_proc),
                    ..Default::default()
                };

                if RegisterClassExW(&wnd_class) == 0 {
                    return Err(format!(
                        "Registering class failed: {}",
                        Error::last_os_error()
                    ));
                }

                let handle = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class_name,
                    w!(""),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    instance,
                    None,
                );

                if handle.0 == 0 {
                    Err(format!(
                        "Message only window creation failed: {}",
                        Error::last_os_error()
                    ))
                } else {
                    Ok(handle)
                }
            };

            handle_result.map(|handle| DummyWindow { handle })
        }
        extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    impl Drop for DummyWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.handle);
            }
        }
    }

    pub fn pump_event_queue() -> bool {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            let mut has_message = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool();
            while msg.message != WM_QUIT && has_message {
                if !IsDialogMessageW(GetAncestor(msg.hwnd, GA_ROOT), &msg).as_bool() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                has_message = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool();
            }

            msg.message == WM_QUIT
        }
    }
}
