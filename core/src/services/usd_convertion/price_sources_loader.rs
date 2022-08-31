use domain::market::MarketId;
use std::collections::HashMap;

use mmb_utils::{cancellation_token::CancellationToken, DateTime};

use domain::order::snapshot::PriceByOrderSide;

#[derive(Default)]
pub struct PriceSourcesLoader {
    // TODO: fix when DatabaseManager will be added
    //database_manager: DatabaseManager
}

impl PriceSourcesLoader {
    pub fn new(//database_manager: DatabaseManager
    ) -> Self {
        Self{
            //database_manager: DatabaseManager
        }
    }

    pub async fn load(
        &self,
        _save_time: DateTime,
        _cancellation_token: CancellationToken,
    ) -> Option<HashMap<MarketId, PriceByOrderSide>> {
        //     const string sqlQuery =
        //         "SELECT a.* FROM public.\"PriceSources\" a " +
        //         "JOIN ( " +
        //         "SELECT \"ExchangeName\", \"CurrencyCodePair\", max(\"DateTime\") \"DateTime\" " +
        //         "FROM public.\"PriceSources\" " +
        //         "WHERE \"DateTime\" <= {0} " +
        //         "GROUP BY \"ExchangeName\", \"CurrencyCodePair\" " +
        //         ") b ON a.\"ExchangeName\" = b.\"ExchangeName\" AND a.\"CurrencyCodePair\" = b.\"CurrencyCodePair\" AND a.\"DateTime\" = b.\"DateTime\"";

        //     await using var session = _databaseManager.Sql;
        //     return await session.Set<PriceSourceModel>()
        //         .FromSqlRaw(sqlQuery, dateTime)
        //         .ToDictionaryAsync(
        //             x => new ExchangeNameSymbol(x.ExchangeName, x.CurrencyCodePair),
        //             x => new PricesBySide(x.Ask, x.Bid),
        //             cancellationToken);

        Some(HashMap::new())
    }
}
