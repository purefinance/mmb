use futures::future::join_all;
use itertools::Itertools;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mockall_double::double;

#[double]
use crate::services::usd_convertion::usd_converter::UsdConverter;

use crate::exchanges::common::Amount;

use super::profit_loss_balance_change::ProfitLossBalanceChange;

pub(crate) fn calculate_raw(profit_loss_balance_changes: &[ProfitLossBalanceChange]) -> Amount {
    profit_loss_balance_changes
        .iter()
        .map(|x| x.usd_balance_change)
        .sum()
}

pub(crate) async fn calculate_over_market(
    profit_loss_balance_changes: &[ProfitLossBalanceChange],
    usd_converter: &UsdConverter,
    cancellation_token: CancellationToken,
) -> Amount {
    let group_by_currency_code = profit_loss_balance_changes
        .iter()
        .into_group_map_by(|x| x.currency_code);

    let usd_converter_actions =
        group_by_currency_code
            .iter()
            .map(|(currency_code, balance_changes)| {
                let sum = balance_changes.iter().map(|x| x.balance_change).sum();
                let cancellation_token = cancellation_token.clone();
                async move {
                    usd_converter
                        .convert_amount(*currency_code, sum, cancellation_token)
                        .await
                        .with_expect(|| {
                            format!("Can't find usd_balance_change for {}", currency_code)
                        })
                }
            });

    join_all(usd_converter_actions).await.iter().sum()
}
