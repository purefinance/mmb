use ibtwsapi::core::contract::Contract;
use mmb_domain::exchanges::symbol::Symbol;

pub fn usstock(symbol: &Symbol) -> Contract {
    let mut contract = Contract::default();

    contract.symbol = symbol.base_currency_id.to_string();
    contract.currency = symbol.quote_currency_id.to_string();

    contract.sec_type = "STK".to_string();
    contract.exchange = "ISLAND".to_string();

    contract
}
