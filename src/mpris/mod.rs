#![allow(clippy::use_self)]

mod metadata;
mod player;
mod playlists;
mod root;
mod tracklist;

use std::error::Error;
use std::sync::Arc;

use log::info;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use zbus::connection;
use zbus::zvariant::ObjectPath;

use crate::application::ASYNC_RUNTIME;
use crate::events::EventManager;
use crate::ext_traits::RwLockExt;
use crate::library::Library;
use crate::queue::Queue;
use crate::spotify::Spotify;

use metadata::build_metadata;
use player::MprisPlayer;
use playlists::MprisPlaylists;
use root::MprisRoot;
use tracklist::MprisTrackList;

/// D-Bus object path for a queue entry with the given stable id.
pub(crate) fn track_path_for_id(id: u64) -> ObjectPath<'static> {
    ObjectPath::from_string_unchecked(format!("/org/ncspot/queue/{id}"))
}

/// The MPRIS "no track" sentinel object path.
pub(crate) fn no_track_path() -> ObjectPath<'static> {
    ObjectPath::from_static_str_unchecked("/org/mpris/MediaPlayer2/TrackList/NoTrack")
}

/// Commands to control the [MprisManager] worker thread.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum MprisCommand {
    /// Emit playback status
    EmitPlaybackStatus,
    /// Emit volume
    EmitVolumeStatus,
    /// Emit metadata
    EmitMetadataStatus,
    /// Emit seeked position
    EmitSeekedStatus(i64),
    /// Emit a LoopStatus PropertiesChanged signal.
    EmitLoopStatus,
    /// Emit a Shuffle PropertiesChanged signal.
    EmitShuffleStatus,
    /// The whole track list changed (clear, bulk add, reorder, activate playlist).
    EmitTrackListReplaced,
    /// A single entry was added; `after_id` is the predecessor entry's id
    /// (None if inserted at the front), captured at insert time.
    EmitTrackAdded { id: u64, after_id: Option<u64> },
    /// The entry with this stable id was removed.
    EmitTrackRemoved(u64),
    /// The metadata of the entry with this stable id changed.
    EmitTrackMetadataChanged(u64),
    /// A playlist changed. Some(id) = that specific playlist; None = bulk refresh.
    EmitPlaylistChanged(Option<String>),
    /// The active playlist changed; emit ActivePlaylist PropertiesChanged.
    EmitActivePlaylistChanged,
}

/// An MPRIS server that internally manager a thread which can be sent commands. This is internally
/// shared and cloning it will yield a reference to the same server.
#[derive(Clone)]
pub struct MprisManager {
    tx: mpsc::UnboundedSender<MprisCommand>,
}

impl MprisManager {
    pub fn new(
        event: EventManager,
        queue: Arc<Queue>,
        library: Arc<Library>,
        spotify: Spotify,
    ) -> Self {
        let root = MprisRoot {};
        let player = MprisPlayer {
            event,
            queue: queue.clone(),
            library: library.clone(),
            spotify: spotify.clone(),
        };
        let tracklist = MprisTrackList {
            queue: queue.clone(),
            library: library.clone(),
            spotify: spotify.clone(),
        };
        let playlist_iface = MprisPlaylists {
            queue,
            library,
            spotify,
        };

        let (tx, rx) = mpsc::unbounded_channel::<MprisCommand>();

        ASYNC_RUNTIME.get().unwrap().spawn(async {
            let result = Self::serve(
                UnboundedReceiverStream::new(rx),
                root,
                player,
                tracklist,
                playlist_iface,
            )
            .await;
            if let Err(e) = result {
                log::error!("MPRIS error: {e}");
            }
        });

        Self { tx }
    }

    async fn serve(
        mut rx: UnboundedReceiverStream<MprisCommand>,
        root: MprisRoot,
        player: MprisPlayer,
        tracklist: MprisTrackList,
        playlist_iface: MprisPlaylists,
    ) -> Result<(), Box<dyn Error + Sync + Send>> {
        let conn = connection::Builder::session()?
            .name(instance_bus_name())?
            .serve_at("/org/mpris/MediaPlayer2", root)?
            .serve_at("/org/mpris/MediaPlayer2", player)?
            .serve_at("/org/mpris/MediaPlayer2", tracklist)?
            .serve_at("/org/mpris/MediaPlayer2", playlist_iface)?
            .build()
            .await?;

        let object_server = conn.object_server();
        let player_iface_ref = object_server
            .interface::<_, MprisPlayer>("/org/mpris/MediaPlayer2")
            .await?;
        let player_iface = player_iface_ref.get().await;
        let tracklist_iface_ref = object_server
            .interface::<_, MprisTrackList>("/org/mpris/MediaPlayer2")
            .await?;
        let playlists_iface_ref = object_server
            .interface::<_, MprisPlaylists>("/org/mpris/MediaPlayer2")
            .await?;

        loop {
            let ctx = player_iface_ref.signal_emitter();
            match rx.next().await {
                Some(MprisCommand::EmitPlaybackStatus) => {
                    player_iface.playback_status_changed(ctx).await?;
                }
                Some(MprisCommand::EmitVolumeStatus) => {
                    info!("sending MPRIS volume update signal");
                    player_iface.volume_changed(ctx).await?;
                }
                Some(MprisCommand::EmitMetadataStatus) => {
                    player_iface.metadata_changed(ctx).await?;
                }
                Some(MprisCommand::EmitSeekedStatus(pos)) => {
                    info!("sending MPRIS seeked signal");
                    MprisPlayer::seeked(ctx, &pos).await?;
                }
                Some(MprisCommand::EmitLoopStatus) => {
                    player_iface.loop_status_changed(ctx).await?;
                }
                Some(MprisCommand::EmitShuffleStatus) => {
                    player_iface.shuffle_changed(ctx).await?;
                }
                Some(MprisCommand::EmitTrackListReplaced) => {
                    let tl_ctx = tracklist_iface_ref.signal_emitter();
                    let tl = tracklist_iface_ref.get().await;
                    let tracks = tl.tracks();
                    let current = tl
                        .queue
                        .get_current_index()
                        .and_then(|i| tl.queue.id_for_index(i))
                        .map(track_path_for_id)
                        .unwrap_or_else(no_track_path);
                    MprisTrackList::track_list_replaced(tl_ctx, tracks, current).await?;
                }
                Some(MprisCommand::EmitTrackAdded { id, after_id }) => {
                    let tl_ctx = tracklist_iface_ref.signal_emitter();
                    let tl = tracklist_iface_ref.get().await;
                    let playable = tl
                        .queue
                        .index_for_id(id)
                        .and_then(|index| tl.queue.queue.read().unwrap().get(index).cloned());
                    if let Some(p) = playable {
                        let spotify = tl.spotify.clone();
                        let library = tl.library.clone();
                        let md = tokio::task::spawn_blocking(move || {
                            build_metadata(Some(&p), track_path_for_id(id), &spotify, &library)
                        })
                        .await
                        .unwrap_or_default();
                        let after = after_id
                            .map(track_path_for_id)
                            .unwrap_or_else(no_track_path);
                        MprisTrackList::track_added(tl_ctx, md, after).await?;
                    }
                }
                Some(MprisCommand::EmitTrackRemoved(id)) => {
                    let tl_ctx = tracklist_iface_ref.signal_emitter();
                    MprisTrackList::track_removed(tl_ctx, track_path_for_id(id)).await?;
                }
                Some(MprisCommand::EmitTrackMetadataChanged(id)) => {
                    let tl_ctx = tracklist_iface_ref.signal_emitter();
                    let tl = tracklist_iface_ref.get().await;
                    if let Some(index) = tl.queue.index_for_id(id) {
                        // Separate the lock read from the await to avoid holding
                        // a RwLockReadGuard across an await point.
                        let p = tl.queue.queue.read().unwrap().get(index).cloned();
                        if let Some(p) = p {
                            let spotify = tl.spotify.clone();
                            let library = tl.library.clone();
                            let md = tokio::task::spawn_blocking(move || {
                                build_metadata(Some(&p), track_path_for_id(id), &spotify, &library)
                            })
                            .await
                            .unwrap_or_default();
                            MprisTrackList::track_metadata_changed(
                                tl_ctx,
                                track_path_for_id(id),
                                md,
                            )
                            .await?;
                        }
                    }
                }
                Some(MprisCommand::EmitPlaylistChanged(id)) => {
                    let pl_ctx = playlists_iface_ref.signal_emitter();
                    let pl = playlists_iface_ref.get().await;
                    match id {
                        Some(id) => {
                            let tuple = {
                                let guard = pl.library.playlists.read_unpoisoned();
                                guard.iter().find(|p| p.id == id).map(|p| {
                                    (
                                        playlists::playlist_path_for_id(&p.id),
                                        p.name.clone(),
                                        p.cover_url.clone().unwrap_or_default(),
                                    )
                                })
                            };
                            if let Some(tuple) = tuple {
                                MprisPlaylists::playlist_changed(pl_ctx, tuple).await?;
                            } else {
                                // Playlist gone (deleted): tell clients the set changed.
                                pl.playlist_count_changed(pl_ctx).await?;
                            }
                        }
                        None => {
                            pl.playlist_count_changed(pl_ctx).await?;
                        }
                    }
                }
                Some(MprisCommand::EmitActivePlaylistChanged) => {
                    let pl_ctx = playlists_iface_ref.signal_emitter();
                    let pl = playlists_iface_ref.get().await;
                    pl.active_playlist_changed(pl_ctx).await?;
                }
                None => break,
            }
        }
        Err("MPRIS server command channel closed".into())
    }

    pub fn send(&self, command: MprisCommand) {
        if let Err(e) = self.tx.send(command) {
            log::warn!("Could not update MPRIS state: {e}");
        }
    }
}

/// Get the D-Bus bus name for this instance according to the MPRIS specification.
///
/// <https://specifications.freedesktop.org/mpris-spec/2.2/#Bus-Name-Policy>
pub fn instance_bus_name() -> String {
    format!(
        "org.mpris.MediaPlayer2.ncspot.instance{}",
        std::process::id()
    )
}
