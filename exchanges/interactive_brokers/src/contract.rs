use ibtwsapi::core::contract::Contract;
use mmb_domain::exchanges::symbol::Symbol;

pub fn usstock(symbol: &Symbol) -> Contract {
    Contract {
        symbol: symbol.base_currency_id.to_string(),
        currency: symbol.quote_currency_id.to_string(),
        sec_type: "STK".to_string(),
        exchange: "ISLAND".to_string(),
        ..Contract::default()
    }
}
