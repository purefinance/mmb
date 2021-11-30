use crate::core::disposition_execution::TradeDisposition;
use crate::core::exchanges::common::Amount;
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;

pub fn is_enough_amount_and_cost(
    disposition: &TradeDisposition,
    amount: Amount,
    need_log: bool,
    symbol: &CurrencyPairMetadata,
) -> Result<(), String> {
    let min_amount = symbol
        .get_min_amount(disposition.price())
        .expect("We can't trade if we can't calculate min amount for order");

    if amount >= min_amount {
        return Ok(());
    }

    let msg = format!(
        "{} Can't create order for amount {} < min amount {} of {}",
        disposition.exchange_account_id(),
        amount,
        min_amount,
        symbol.amount_currency_code
    );

    if need_log {
        log::trace!("{}", msg);
    }

    return Err(msg);
}
