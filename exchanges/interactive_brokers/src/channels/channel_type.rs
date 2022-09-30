use function_name::named;
use ibtwsapi::core::messages::ServerRspMsg;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ChannelType {
    CancelOrder,
    CreateOrder,
    GetBalance,
    GetMyTrades,
    GetOpenOrders,
    GetPositions,
}

impl ChannelType {
    pub fn get_all() -> &'static [Self; 6] {
        &[
            Self::CancelOrder,
            Self::CreateOrder,
            Self::GetBalance,
            Self::GetMyTrades,
            Self::GetOpenOrders,
            Self::GetPositions,
        ]
    }

    #[named]
    pub fn from_msg(msg: &ServerRspMsg) -> &'static [Self] {
        let f_n = function_name!();

        match &msg {
            ServerRspMsg::AccountSummary { .. } => &[Self::GetBalance],
            ServerRspMsg::AccountSummaryEnd { .. } => &[Self::GetBalance],
            ServerRspMsg::CompletedOrder { .. } => &[Self::GetMyTrades],
            ServerRspMsg::CompletedOrdersEnd => &[Self::GetMyTrades],
            ServerRspMsg::ErrMsg { .. } => Self::get_all(),
            ServerRspMsg::OpenOrder { .. } => &[Self::CreateOrder, Self::GetOpenOrders],
            ServerRspMsg::OpenOrderEnd => &[Self::GetOpenOrders],
            ServerRspMsg::OrderStatus { .. } => &[Self::CancelOrder],
            ServerRspMsg::PositionData { .. } => &[Self::GetPositions],
            ServerRspMsg::PositionEnd { .. } => &[Self::GetPositions],
            _ => {
                log::debug!("fn {f_n}: received unsupported message: {:?}.", msg);

                &[]
            }
        }
    }
}
