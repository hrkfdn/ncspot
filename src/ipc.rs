use std::{io, path::PathBuf};

use log::{debug, error, info};
use tokio::net::{UnixListener, UnixStream};
use tokio::runtime::Handle;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use crate::events::{Event, EventManager};

pub struct IpcSocket {
    listener: UnixListener,
    ev: EventManager,
}

impl IpcSocket {
    pub fn new(handle: &Handle, path: PathBuf, ev: EventManager) -> io::Result<IpcSocket> {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }

        info!("Creating IPC domain socket at {path:?}");

        let _guard = handle.enter();
        let listener = UnixListener::bind(path)?;
        Ok(IpcSocket { listener, ev })
    }

    pub async fn worker(&self) {
        loop {
            match self.listener.accept().await {
                Ok((stream, sockaddr)) => {
                    debug!("Connection from {:?}", sockaddr);
                    tokio::spawn(Self::stream_handler(stream, self.ev.clone()));
                }
                Err(e) => error!("Error accepting connection: {e}"),
            }
        }
    }

    async fn stream_handler(mut stream: UnixStream, ev: EventManager) -> io::Result<()> {
        let (reader, _writer) = stream.split();
        let mut framed_reader = FramedRead::new(reader, LinesCodec::new());

        loop {
            if let Some(line) = framed_reader.next().await {
                match line {
                    Ok(line) => {
                        debug!("Received line: \"{line}\"");
                        ev.send(Event::IpcInput(line));
                    }
                    Err(e) => error!("Error reading line: {e}"),
                }
            } else {
                debug!("Closing connection");
                return Ok(());
            }
        }
    }
}
