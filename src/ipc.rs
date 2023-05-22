use std::{io, path::PathBuf};

use futures::SinkExt;
use log::{debug, error, info};
use tokio::net::{UnixListener, UnixStream};
use tokio::runtime::Handle;
use tokio::sync::watch::{Receiver, Sender};
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

use crate::events::{Event, EventManager};
use crate::model::playable::Playable;
use crate::spotify::PlayerEvent;

pub struct IpcSocket {
    tx: Sender<Status>,
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct Status {
    mode: PlayerEvent,
    playable: Option<Playable>,
}

impl Drop for IpcSocket {
    fn drop(&mut self) {
        log::info!("Removing IPC socket: {:?}", self.path);
        std::fs::remove_file(&self.path).expect("Could not remove IPC socket");
    }
}

impl IpcSocket {
    pub fn new(handle: &Handle, path: PathBuf, ev: EventManager) -> io::Result<IpcSocket> {
        let path = if path.exists() && Self::is_open_socket(&path) {
            let mut new_path = path;
            new_path.set_file_name(format!("ncspot.{}.sock", std::process::id()));
            new_path
        } else if path.exists() && !Self::is_open_socket(&path) {
            std::fs::remove_file(&path)?;
            path
        } else {
            path
        };

        info!("Creating IPC domain socket at {path:?}");

        let status = Status {
            mode: PlayerEvent::Stopped,
            playable: None,
        };

        let (tx, rx) = tokio::sync::watch::channel(status);
        let listener_path = path.clone();
        handle.spawn(async move {
            let listener =
                UnixListener::bind(listener_path).expect("Could not create IPC domain socket");
            Self::worker(listener, ev, rx.clone()).await;
        });

        Ok(IpcSocket { tx, path })
    }

    fn is_open_socket(path: &PathBuf) -> bool {
        std::os::unix::net::UnixStream::connect(path).is_ok()
    }

    pub fn publish(&self, event: &PlayerEvent, playable: Option<Playable>) {
        let status = Status {
            mode: event.clone(),
            playable,
        };
        self.tx.send(status).expect("Error publishing IPC update");
    }

    async fn worker(listener: UnixListener, ev: EventManager, tx: Receiver<Status>) {
        loop {
            match listener.accept().await {
                Ok((stream, sockaddr)) => {
                    debug!("Connection from {:?}", sockaddr);
                    tokio::spawn(Self::stream_handler(
                        stream,
                        ev.clone(),
                        WatchStream::new(tx.clone()),
                    ));
                }
                Err(e) => error!("Error accepting connection: {e}"),
            }
        }
    }

    async fn stream_handler(
        mut stream: UnixStream,
        ev: EventManager,
        mut rx: WatchStream<Status>,
    ) -> Result<(), String> {
        let (reader, writer) = stream.split();
        let mut framed_reader = FramedRead::new(reader, LinesCodec::new());
        let mut framed_writer = FramedWrite::new(writer, LinesCodec::new());

        loop {
            tokio::select! {
                line = framed_reader.next() => {
                    match line {
                        Some(Ok(line)) => {
                            debug!("Received line: \"{line}\"");
                            ev.send(Event::IpcInput(line));
                        }
                        Some(Err(e)) => error!("Error reading line: {e}"),
                        None => {
                            debug!("Closing IPC connection");
                            return Ok(())
                        }
                    }
                }
                Some(status) = rx.next() => {
                    debug!("IPC Status update: {status:?}");
                    let status_str = serde_json::to_string(&status).map_err(|e| e.to_string())?;
                    framed_writer.send(status_str).await.map_err(|e| e.to_string())?;
                }
                else => {
                    error!("All streams are closed");
                    return Ok(())
                }
            }
        }
    }
}
