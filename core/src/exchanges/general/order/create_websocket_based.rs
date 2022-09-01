use mmb_domain::order::fill::EventSourceType;
use mmb_domain::order::snapshot::{ClientOrderId, ExchangeOrderId};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use tokio::sync::oneshot;

use crate::{exchanges::general::exchange::Exchange, exchanges::general::exchange::RequestResult};
use mmb_domain::order::pool::OrderRef;

use super::create::CreateOrderResult;

impl Exchange {
    pub(super) async fn create_order_core(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Option<CreateOrderResult> {
        let client_order_id = order.client_order_id();
        let (tx, mut websocket_event_receiver) = oneshot::channel();

        // TODO insert is not analog of C# GetOrAd!
        // Here has to be entry().or_insert()
        self.order_creation_events
            .insert(client_order_id.clone(), (tx, None));

        let create_order_future = self.exchange_client.create_order(order);

        tokio::select! {
            create_order_result = create_order_future => {
                match create_order_result.outcome {
                    RequestResult::Error(_) => {
                        // TODO if ExchangeFeatures.Order.CreationResponseFromRestOnlyForError
                        Some(create_order_result)
                    }
                    RequestResult::Success(_) => tokio::select! {
                            websocket_outcome = &mut websocket_event_receiver => websocket_outcome.ok(),
                            _ = cancellation_token.when_cancelled() => None,
                        },
                }
            }
            _ = cancellation_token.when_cancelled() => None,
            websocket_outcome = &mut websocket_event_receiver => websocket_outcome.ok(),
        }
    }

    pub(crate) fn raise_order_created(
        &self,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
        source_type: EventSourceType,
    ) {
        if let Some((_, (tx, _))) = self.order_creation_events.remove(client_order_id) {
            if let Err(error) = tx.send(CreateOrderResult::succeed(exchange_order_id, source_type))
            {
                log::error!("Unable to send thru oneshot channel: {error:?}");
            }
        } else {
            self.handle_create_order_succeeded(
                self.exchange_account_id,
                client_order_id,
                exchange_order_id,
                source_type,
            )
            .with_expect(|| format!("Error handle create order succeeded for {client_order_id:?}"));
        }
    }
}
