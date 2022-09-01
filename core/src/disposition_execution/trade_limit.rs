use crate::disposition_execution::TradeDisposition;
use mmb_domain::exchanges::symbol::Symbol;
use mmb_domain::order::snapshot::Amount;

pub fn is_enough_amount_and_cost(
    disposition: &TradeDisposition,
    amount: Amount,
    need_log: bool,
    symbol: &Symbol,
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

    Err(msg)
}
