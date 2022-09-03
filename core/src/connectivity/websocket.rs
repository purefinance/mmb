use super::websocket_connection::open_connection;
use super::{ConnectivityError, Result, WebSocketParams, WebSocketRole};
use crate::infrastructure::spawn_future;
use futures::FutureExt;
use mmb_domain::market::ExchangeAccountId;
use mmb_utils::infrastructure::SpawnFutureFlags;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::{CancellationToken, DropGuard as CancellationTokenDropGuard};

pub struct WsSender {
    /// Main websocket connection sender
    main_sender: mpsc::UnboundedSender<Message>,
    /// Secondary websocket connection sender
    secondary_sender: Option<mpsc::UnboundedSender<Message>>,
    /// Cancellation token for service futures
    _cancel: CancellationTokenDropGuard,
}

/// Websocket send end wrapper
impl WsSender {
    /// Send to main websocket
    pub fn send_main(&self, msg: String) -> Result<()> {
        self.main_sender
            .send(Message::Text(msg))
            .map_err(|_| ConnectivityError::NotConnected)
    }

    /// Send to secondary websocket
    pub fn send_secondary(&self, msg: String) -> Result<()> {
        self.secondary_sender
            .as_ref()
            .ok_or(ConnectivityError::SecondaryConnectorIsNotPresent)?
            .send(Message::Text(msg))
            .map_err(|_| ConnectivityError::NotConnected)
    }
}

pub async fn websocket_open(
    exchange_account_id: ExchangeAccountId,
    main: WebSocketParams,
    secondary: Option<WebSocketParams>,
) -> Result<(WsSender, mpsc::UnboundedReceiver<String>)> {
    log::trace!("Websocket '{}' connecting", exchange_account_id);
    let ret = if let Some(secondary) = secondary {
        connect_both_parallel(main, secondary, exchange_account_id).await?
    } else {
        connect_only_main(main, exchange_account_id).await?
    };
    log::trace!("Websocket '{}' connected", exchange_account_id);
    Ok(ret)
}

async fn connect_both_parallel(
    main: WebSocketParams,
    secondary: WebSocketParams,
    exchange_account_id: ExchangeAccountId,
) -> Result<(WsSender, mpsc::UnboundedReceiver<String>)> {
    let cancel = CancellationToken::new();
    let (main, secondary) = tokio::join!(
        open_connection(
            exchange_account_id,
            WebSocketRole::Main,
            main,
            cancel.clone()
        ),
        open_connection(
            exchange_account_id,
            WebSocketRole::Secondary,
            secondary,
            cancel.clone()
        )
    );

    match (main, secondary) {
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(main), Ok(secondary)) => {
            let chan = mpsc::unbounded_channel();
            let sender = WsSender {
                main_sender: main.0,
                secondary_sender: Some(secondary.0),
                _cancel: cancel.drop_guard(),
            };
            spawn_future(
                "spawn combined_channel_reader",
                SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                async move {
                    combined_channel_reader(main.1, secondary.1, chan.0).await;
                    Ok(())
                }
                .boxed(),
            );
            Ok((sender, chan.1))
        }
    }
}

/// Forward input from two channels to single output
async fn combined_channel_reader(
    mut main: mpsc::UnboundedReceiver<String>,
    mut secondary: mpsc::UnboundedReceiver<String>,
    tx: mpsc::UnboundedSender<String>,
) {
    loop {
        // init message from main or secondary channel
        // finish processing when one of the channels closed
        let message = tokio::select! {
            m = main.recv() => m,
            m = secondary.recv() => m
        };

        let message = match message {
            None => break, // loop
            Some(m) => m,
        };

        if tx.send(message).is_err() {
            // can't forward message, no receiver
            break;
        }
    }
}

/// Connect single (main) socket
async fn connect_only_main(
    params: WebSocketParams,
    exchange_account_id: ExchangeAccountId,
) -> Result<(WsSender, mpsc::UnboundedReceiver<String>)> {
    let cancel = CancellationToken::new();
    let (tx, rx) = open_connection(
        exchange_account_id,
        WebSocketRole::Main,
        params,
        cancel.clone(),
    )
    .await?;
    let sender = WsSender {
        main_sender: tx,
        secondary_sender: None,
        _cancel: cancel.drop_guard(),
    };
    Ok((sender, rx))
}
