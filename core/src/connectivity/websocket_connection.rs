use super::{ConnectivityError, Result, WebSocketParams, WebSocketRole};
use crate::infrastructure::spawn_future_ok;
use domain::market::ExchangeAccountId;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use mmb_utils::infrastructure::SpawnFutureFlags;
use std::fmt::Formatter;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{timeout, timeout_at, Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

/// Time interval between heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Time interval before lack of client response causes a timeout
const HEARTBEAT_FAIL_TIMEOUT: Duration = Duration::from_secs(10);

/// Write deadline
///
/// This timeout is for networking buffer overflow detection.  
const WRITE_DEADLINE_TIMEOUT: Duration = Duration::from_secs(1);

const PING_MESSAGE: &[u8; 9] = b"heartbeat";

type TrySendResult = std::result::Result<(), mpsc::error::TrySendError<Message>>;

/// Compound log records key
#[derive(Copy, Clone)]
struct Meta(ExchangeAccountId, WebSocketRole);

impl std::fmt::Display for Meta {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

type WebSocketWriter = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
type WebSocketReader = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

/// Websocket writer
struct WriterHandle {
    /// WS writer handle
    writer: WebSocketWriter,
    /// For pretty logs
    meta: Meta,
    /// Channel from `ReaderHandle`
    internal_rx: mpsc::Receiver<Message>,
    /// User's input channel
    writer_rx: mpsc::UnboundedReceiver<Message>,
    /// Cancellation token.
    ///
    /// This one is bidirectional: we use it to trigger signal and to wait for the signal from
    /// another source  
    cancel: CancellationToken,
}

impl WriterHandle {
    /// Main loop
    async fn run(mut self) {
        // To fire cancellation signal at exit unconditionally
        let cancel = self.cancel.clone().drop_guard();

        loop {
            // select message from two channels or break the loop if one of them was exhausted
            // also respects cancellation
            let message_to_send = tokio::select! {
                biased; // check in provided order

                _ = self.cancel.cancelled() => {
                    log::trace!(
                        "Websocket {} writer received cancel during polling for messages",
                        self.meta
                    );
                    break
                }

                msg = self.internal_rx.recv() => {
                    match msg {
                        Some(msg) => msg,
                        None => {
                            log::trace!(
                                "Websocket {} writer received shutdown from reader future",
                                self.meta
                            );
                            break
                        }
                    }
                }

                msg = self.writer_rx.recv() => {
                    match msg {
                        Some(msg) => msg,
                        None => {
                            log::trace!(
                                "Websocket {} writer received shutdown from sender channel",
                                self.meta
                            );
                            break
                        }
                    }
                },
            };

            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => {
                    log::trace!(
                        "Websocket {} writer received cancel during polling for messages",
                        self.meta
                    );
                    break
                }

                e = timeout(WRITE_DEADLINE_TIMEOUT, self.writer.send(message_to_send)) => {
                    match e {
                        Ok(Ok(())) => continue,
                        Ok(Err(e)) => {
                            log::error!("Websocket {} writer send failed: {:?}", self.meta, e);
                            // previous send failed, not able to send soft close
                            // trying to hard close then
                            let _ = self.writer.close().await;
                            return
                        }
                        Err(_) => {
                            log::error!(
                                "Websocket {} writer received cancel during send",
                                self.meta
                            );
                            return
                        }
                    }
                }
            };
        } // loop

        // drop all channels to notify dependent futures
        let WriterHandle {
            mut writer, meta, ..
        } = self;
        // also fire cancel signal before graceful websocket shutdown
        drop(cancel);

        // we can block here for a while, but it's not an issue
        // try to send force close and close
        let _ = writer.send(Message::Close(None)).await;
        let _ = writer.close().await;

        log::debug!("Websocket {} writer finished", meta);
    }
}

/// Websocket reader
struct ReaderHandle {
    /// WS reader handle
    reader: WebSocketReader,
    /// For pretty logs
    meta: Meta,
    /// Channel to user
    reader_tx: mpsc::UnboundedSender<String>,
    /// Channel to `WriterHandle`
    internal_tx: mpsc::Sender<Message>,
    /// Cancellation token.
    ///
    /// This one is bidirectional: we use it to trigger signal and to wait for the signal from
    /// another source  
    cancel: CancellationToken,
}

impl ReaderHandle {
    /// Main loop
    async fn run(mut self) {
        // To fire cancellation signal at exit
        let _cancel = self.cancel.clone().drop_guard();
        // receive date time point
        let mut receive_ts = Instant::now();
        // next heartbeat time point
        let mut next_heartbeat_ts = receive_ts + HEARTBEAT_INTERVAL;

        loop {
            let result = tokio::select! {
                biased; // check in provided order
                _ = self.cancel.cancelled() => {
                    log::debug!("Websocket {} reader received cancel signal", self.meta);
                    break;
                }
                res = timeout_at(next_heartbeat_ts, self.reader.next()) => res,
            };

            // wrap receive in heartbeat timer
            // send heartbeats only if no data was received for HEARTBEAT_INTERVAL
            let msg = match result {
                Ok(Some(Err(e))) => {
                    log::error!("Websocket {} reader recv failure: {:?}", self.meta, e);
                    return;
                }

                Ok(Some(Ok(msg))) => {
                    // received data, will process it after match statement
                    msg
                }

                Ok(None) => {
                    // clean close
                    log::debug!("Websocket {} reader received oef", self.meta);
                    break;
                }

                Err(_) => {
                    // heartbeat timeout
                    if receive_ts.elapsed() >= HEARTBEAT_FAIL_TIMEOUT {
                        log::error!("Websocket {} reader reached heartbeat deadline", self.meta);
                        return;
                    }
                    // will send heartbeat again after HEARTBEAT_INTERVAL
                    next_heartbeat_ts += HEARTBEAT_INTERVAL;

                    if (self.send_ping()).is_err() {
                        log::error!("Websocket {} reader failed to send ping", self.meta);
                        return;
                    };
                    continue;
                }
            };

            // received message processing
            receive_ts = Instant::now();
            next_heartbeat_ts = receive_ts + HEARTBEAT_INTERVAL;

            match msg {
                Message::Text(text) => {
                    if self.forward_message(text).is_err() {
                        log::trace!(
                            "Websocket {} reader failed to forward message, exiting",
                            self.meta
                        );
                        return;
                    }
                }
                Message::Binary(bytes) => log::trace!(
                    "Websocket {} reader received binary message: {bytes:x?}",
                    self.meta,
                ),
                Message::Ping(msg) => {
                    if (self.send_pong(Message::Pong(msg))).is_err() {
                        log::trace!(
                            "Websocket {} reader failed to send ping, exiting",
                            self.meta
                        );
                        return;
                    }
                }
                Message::Pong(_) => {
                    // we don't care about it's content
                }
                Message::Close(reason) => {
                    log::trace!(
                        "Websocket {} reader received close with reason: {reason:?}",
                        self.meta
                    );

                    break;
                }
                Message::Frame(frame) => log::trace!(
                    "Websocket {} reader received close with reason: {frame:?}",
                    self.meta
                ),
            }
        }
        log::debug!("Websocket {} reader finished", self.meta);
    }

    fn send_ping(&self) -> TrySendResult {
        log::trace!("Websocket {} reader triggers ping packet", self.meta);
        self.internal_tx
            .try_send(Message::Ping(PING_MESSAGE.to_vec()))
    }

    fn send_pong(&self, msg: Message) -> TrySendResult {
        log::trace!("Websocket {} reader sending pong message", self.meta);
        self.internal_tx.try_send(msg)
    }

    /// Forward websocket message to the user
    fn forward_message(
        &self,
        msg: String,
    ) -> std::result::Result<(), mpsc::error::SendError<String>> {
        self.reader_tx.send(msg)
    }
}

/// Open WebSocket connection.
///
/// Provided cancellation token can be used to shutdown service futures instantly.
///
/// # Return
/// Tuple: (send channel, read channel)
pub async fn open_connection(
    exchange_account_id: ExchangeAccountId,
    role: WebSocketRole,
    params: WebSocketParams,
    cancel: CancellationToken,
) -> Result<(
    mpsc::UnboundedSender<Message>,
    mpsc::UnboundedReceiver<String>,
)> {
    let (ws_stream, _) = connect_async(params.url.clone())
        .await
        .map_err(|e| ConnectivityError::FailedToConnect(role, params.url.to_string(), e))?;

    let meta = Meta(exchange_account_id, role);

    let (writer_tx, writer_rx) = mpsc::unbounded_channel();
    let (internal_tx, internal_rx) = mpsc::channel(1);
    let (reader_tx, reader_rx) = mpsc::unbounded_channel();

    let (writer, reader) = ws_stream.split();
    let writer = WriterHandle {
        writer,
        meta,
        internal_rx,
        writer_rx,
        cancel: cancel.clone(),
    };

    let reader = ReaderHandle {
        reader,
        meta,
        internal_tx,
        reader_tx,
        cancel,
    };

    spawn_future_ok(
        "WriterHandler::run",
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        writer.run(),
    );
    spawn_future_ok(
        "ReaderHandle::run",
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        reader.run(),
    );

    Ok((writer_tx, reader_rx))
}
