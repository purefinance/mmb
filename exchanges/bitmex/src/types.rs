use anyhow::bail;
use mmb_domain::market::SpecificCurrencyPair;
use mmb_domain::order::snapshot::{Amount, ClientOrderId, ExchangeOrderId, OrderSide, Price};
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use serde::{de, Deserialize, Deserializer};
use std::fmt;
use std::fmt::Debug;

/// Bitmex Symbol description
/// {
/// "symbol": "string", // The contract for this position.
/// "rootSymbol": "string",// Root symbol for the instrument, used for grouping on the frontend.
/// "state": "string", // State of the instrument, it can be `Open`Closed`Unlisted`Expired`Cleared.
/// "typ": "string", // Type of the instrument (e.g. Futures, Perpetual Contracts).
/// "listing": "2020-09-14T02:26:42.916Z",// Date when the instrument was listed.
/// "front": "2020-09-14T02:26:42.916Z", // Front month time.
/// "expiry": "2020-09-14T02:26:42.916Z", // Date when the instrument is expired.
/// "settle": "2020-09-14T02:26:42.916Z", // Date when the instrument is expired (always the same as expiry).
/// "relistInterval": "2020-09-14T02:26:42.916Z", // Depreciated - The time between two consecutive listings. Only applies to UPs & DOWNs, that are settled and relisted every 7 days.
/// "inverseLeg": "string", // Depreciated - Instrument that this instrument will merge to (legacy).
/// "sellLeg": "string",// Depreciated - Sell leg of the calendar spreads (legacy).
/// "buyLeg": "string",// Depreciated - Buy leg of the calendar spreads (legacy).
/// "optionStrikePcnt": 0, // Depreciated - UPs and DOWNs only. Strike percent
/// "optionStrikeRound": 0, // Depreciated - UPs and DOWNs only. Strike round, always 250
/// "optionStrikePrice": 0, // Depreciated - UPs and DOWNs only. Strike price
/// "optionMultiplier": 0, // Depreciated - UPs and DOWNs only. Contract multiplier
/// "positionCurrency": "string", // Currency for position of this contract. If not null, 1 contract = 1 positionCurrency.
/// "underlying": "string", // Defines the underlying asset of the instrument (e.g.XBT).
/// "quoteCurrency": "string", // Currency of the quote price.
/// "underlyingSymbol": "string", // Symbol of the underlying asset.
/// "reference": "string", // Venue of the reference symbol.
/// "referenceSymbol": "string", // Symbol of index being referenced (e.g. .BXBT).
/// "calcInterval": "2020-09-14T02:26:42.916Z", // The time between two consecutive calculations. Only available for BTMX reference indices.
/// "publishInterval": "2020-09-14T02:26:42.916Z", // The time between two consecutive publish. Available for all indices.
/// "publishTime": "2020-09-14T02:26:42.916Z" // The time of the publish. Only available for BTMX reference indices.
/// "maxOrderQty": 0, // Maximum order quantity - Contract specific and applies to contracts only.
/// "maxPrice": 0, // Maximum price - Applies to contracts only.
/// "lotSize": 0, // Lot size - The minimum unit in an order quantity - Applies to contracts only.
/// "tickSize": 0,// Minimum price movement of a trading instrument.
/// "multiplier": 0, // contract specific multiplier which determines the worth of the contract.
/// "settlCurrency": "string",// Currency that PnL is denominated in (e.g. XBT).
/// "underlyingToPositionMultiplier": 0, // Multiplier from underlying currency to position currency.
/// "underlyingToSettleMultiplier": 0, // Multiplier from underlying currency to settle currency.
/// "quoteToSettleMultiplier": 0, // Multiplier from quote currency to settle currency.
/// "isQuanto": true, // Is the contract quanto or not.
/// "isInverse": true, // Is the contract inverse or not.
/// "initMargin": 0 // Initial margin requirement - Contract specific.
/// "maintMargin": 0, // Maintenance margin requirement - Contract specific.
/// "riskLimit": 0, // The max-leverage Risk Limit for this instrument.
/// "riskStep": 0, // When increasing your Risk Limit, the Risk Step is the size that the Risk Limit increases per multiple of the maintenance margin.
/// "limit": 0, // The limit of daily price change.
/// "capped": true, // Whether it's capped or not
/// "taxed": true, // Depreciated - whether it can be taxed (legacy)
/// "deleverage": true, // whether it can be deleveraged
/// "makerFee": 0, // Maker Fee (-0.0250%).
/// "takerFee": 0, // Taker Fee (0.0750%).
/// "settlementFee": 0, // Settlement Fee rate.
/// "insuranceFee": 0, // Depreciated - Insurance fee rate (legacy).
/// "fundingBaseSymbol": "string", // Funding base currency. (Only applies to quanto contracts)
/// "fundingQuoteSymbol": "string", // Funding quote currency. (Only applies to quanto contracts)
/// "fundingPremiumSymbol": "string", // Funding premium index. (Only applies to quanto contracts)
/// "fundingTimestamp": "2020-09-14T02:26:42.916Z", // Next funding time. (Only applies to quanto contracts)
/// "fundingInterval": "2020-09-14T02:26:42.916Z", // The time between two consecutive fundings. (Only applies to quanto contracts)
/// "fundingRate": 0, // The funding rate (if applicable) of the instrument.
/// "indicativeFundingRate": 0, // Indicative funding rate for the next 8 hour period.
/// "rebalanceTimestamp": "2020-09-14T02:26:42.916Z", // Depreciated - Next rebalance time (legacy).
/// "rebalanceInterval": "2020-09-14T02:26:42.916Z", // Depreciated - The time between two consecutive rebalances (legacy).
/// "openingTimestamp": "2020-09-14T02:26:42.916Z", // Opening timestamp of the last trading session of this contract.
/// "closingTimestamp": "2020-09-14T02:26:42.916Z", // Closing timestamp of the last trading session of this contract.
/// "sessionInterval": "2020-09-14T02:26:42.916Z", // Session interval of this contract.
/// "prevClosePrice": 0, // Close price of the previous trading session.
/// "limitDownPrice": 0, // Down limit of the order price.
/// "limitUpPrice": 0, // Up limit of the order price.
/// "bankruptLimitDownPrice": 0, // Depreciated - Legacy.
/// "bankruptLimitUpPrice": 0, // Depreciated - Legacy.
/// "prevTotalVolume": 0, // Lifetime volume up to this session start time.
/// "totalVolume": 0, // Lifetime volume.
/// "volume": 0, // Volume of the current trading session.
/// "volume24h": 0, // 24 hour volume, sum size (trade table) i.e. lastQty from execution
/// "prevTotalTurnover": 0, // Lifetime turnover up to this session start time.
/// "totalTurnover": 0, // Lifetime turnover.
/// "turnover": 0, // Turnover of the current trading session
/// "turnover24h": 0,// 24 hour turnover, sum grossValue (trade table) i.e. abs execCost from execution.
/// "homeNotional24h": 0, // The volume24hr of underlying currency.
/// "foreignNotional24h": 0, // The volume24hr of quote currency.
/// "prevPrice24h": 0, // Price of 24 hour ago.
/// "vwap": 0, // Volume weighted average price of the last 24 hours.
/// "highPrice": 0, // Highest price in the last 24hrs.
/// "lowPrice": 0, // Lowest price in the last 24hrs.
/// "lastPrice": 0, // Last price.
/// "lastPriceProtected": 0, // Last price protected by price band (see https://www.bitmex.com/app/fairPriceMarking#Last-Price-Protected-Marking)
/// "lastTickDirection": "string", // The relationship between the last trade’s price and the previous ones (MinusTick, ZeroMinusTick, ZeroPlusTick, PlusTick).
/// "lastChangePcnt": 0, // Change percentage in the past 24hrs.
/// "bidPrice": 0, // Last bid price.
/// "midPrice": 0, // Average price of bidPrice and askPrice.
/// "askPrice": 0, // Last ask price.
/// "impactBidPrice": 0, // Impact bid brice (see https://www.bitmex.com/app/fairPriceMarking#Impact-Bid-Ask-and-Mid-Price).
/// "impactMidPrice": 0, // Impact mid price.
/// "impactAskPrice": 0, // Impact ask price.
/// "hasLiquidity": true, // Whether the impact bid and ask prices are within one maintenance margin percentage.
/// "openInterest": 0, // Open interest in terms of number of contracts.
/// "openValue": 0, // The open value in the settlement currency of the contract.
/// "fairMethod": "string", //  Method used for fair price calculation, it can be FundingRate or ImpactMidPrice.
/// "fairBasisRate": 0, // Fair basis rate annualised.
/// "fairBasis": 0, // Fair basis.
/// "fairPrice": 0, // The fair price of the instrument.
/// "markMethod": "string", // Method used for mark price, it can be FairPrice or LastPrice or LastPriceProtected.
/// "markPrice": 0, // Mark price.
/// "indicativeTaxRate": 0, // Depreciated - Indicative tax rate (legacy).
/// "indicativeSettlePrice": 0, // This is the price of the index associated with the instrument.
/// "optionUnderlyingPrice": 0, // The price of the underlying asset for option. Option underlying price. Null for others.
/// "settledPrice": 0, // Settled price, for settled contracts. Null for others
/// "timestamp": "2020-09-14T02:26:42.917Z" // Timestamp
/// }
#[derive(Deserialize, Debug)]
pub(crate) struct BitmexSymbol<'a> {
    #[serde(rename = "typ")]
    pub(crate) symbol_type: &'a str,
    #[serde(rename = "symbol")]
    pub(crate) id: &'a str,
    #[serde(rename = "underlying")]
    pub(crate) base_id: &'a str,
    #[serde(rename = "quoteCurrency")]
    pub(crate) quote_id: &'a str,
    pub(crate) state: &'a str,
    #[serde(rename = "tickSize")]
    pub(crate) price_tick: Decimal,
    #[serde(rename = "lotSize")]
    pub(crate) amount_tick: Decimal,
    #[serde(rename = "maxPrice")]
    pub(crate) max_price: Option<Price>,
    #[serde(rename = "maxOrderQty")]
    pub(crate) max_amount: Option<Amount>,
}

#[derive(PartialEq)]
pub(crate) enum BitmexSymbolType {
    PerpetualContract,
    PerpetualContractFXUnderlier,
    Spot,
    Future,
    BasketIndex,
    CryptoIndex,
    FXIndex,
    LendingIndex,
    VolatilityIndex,
}

impl BitmexSymbolType {
    fn as_str(&self) -> &str {
        match self {
            BitmexSymbolType::PerpetualContract => "FFWCSX",
            BitmexSymbolType::PerpetualContractFXUnderlier => "FFWCSF",
            BitmexSymbolType::Spot => "IFXXXP",
            BitmexSymbolType::Future => "FFCCSX",
            BitmexSymbolType::BasketIndex => "MRBXXX",
            BitmexSymbolType::CryptoIndex => "MRCXXX",
            BitmexSymbolType::FXIndex => "MRFXXX",
            BitmexSymbolType::LendingIndex => "MRRXXX",
            BitmexSymbolType::VolatilityIndex => "MRIXXX",
        }
    }
}

impl Debug for BitmexSymbolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for BitmexSymbolType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "FFWCSX" => Ok(Self::PerpetualContract),
            "FFWCSF" => Ok(Self::PerpetualContractFXUnderlier),
            "IFXXXP" => Ok(Self::Spot),
            "FFCCSX" => Ok(Self::Future),
            "MRBXXX" => Ok(Self::BasketIndex),
            "MRCXXX" => Ok(Self::CryptoIndex),
            "MRFXXX" => Ok(Self::FXIndex),
            "MRRXXX" => Ok(Self::LendingIndex),
            "MRIXXX" => Ok(Self::VolatilityIndex),
            _ => bail!("Unknown symbol type"),
        }
    }
}

/// Bitmex wallet asset description
///{
///"asset": "XBT",
///"currency": "XBt",
///"majorCurrency": "XBT",
///"name": "Bitcoin",
///"currencyType": "Crypto",
///"scale": 8,
///"enabled": true,
///"isMarginCurrency": true,
///"networks": [
///             {
///                 "asset": "BTC",
///                 "tokenAddress": "",
///                 "depositEnabled": true,
///                 "withdrawalEnabled": true,
///                 "withdrawalFee": 0,
///                 "minFee": 0,
///                 "maxFee": 0
///             }
///            ]
///}
#[derive(Deserialize, Debug)]
pub(crate) struct BitmexWalletAsset<'a> {
    #[serde(rename = "majorCurrency")]
    pub(crate) currency: &'a str,
    pub(crate) scale: u8,
}

/// Bitmex order info request result. Note than price and amount fields are optional and they are null when rejected or canceled order was received
/// {
/// "orderID": "string", // Unique identifier for Order as assigned by (BitMEX).
/// "clOrdID": "string", // clOrdID refers to "Client Order ID" which is an optional field that you can use to personally identify your open orders.
/// "clOrdLinkID": "string", // Permits order originators to tie together groups of orders in which trades resulting from orders are associated for a specific purpose, for example the calculation of average execution price for a customer or to associate lists submitted to a broker as waves of a larger program trade.
/// "account": 0,// Your unique account ID.
/// "symbol": "string", // The contract for this position.
/// "side": "string",// Side of order.
/// "simpleOrderQty": 0, // Depreciated
/// "orderQty": 0, // Quantity Ordered
/// "price": 0,// Price of contract
/// "displayQty": 0, // The quantity to be displayed . Required for reserve orders. On orders specifies the qty to be displayed, on execution reports the currently displayed quantity.
/// "stopPx": 0, // Price of contract
/// "pegOffsetValue": 0, // Amount (signed) added to the peg for a pegged order in the context of the  PegOffsetType (836) (Prior to FIX 4.4 this field was of type PriceOffset)
/// "pegPriceType": "string", // Defines the type of peg.
/// "currency": "string", // Identifies currency used for price.
/// "settlCurrency": "string", // The settlement currency of the contract
/// "ordType": "string", // Order Type
/// "timeInForce": "string", // Specifies how long the order remains in effect. Absence of this field is interpreted as DAY.
/// "execInst": "string", // Instructions for order handling on exchange trading floor. If more than one instruction is applicable to an order, this field can contain multiple instructions separated by space.
/// "contingencyType": "string", // Depreciated
/// "exDestination": "string", // Execution destination as defined by institution when order is entered.
/// "ordStatus": "string", // Identifies current status of order
/// "triggered": "string", // Indication of whether an order is triggered or not (e.g "StopOrderTriggered").
/// "workingIndicator": true,// Indicates if the order is currently being worked.
/// "ordRejReason": "string",// Code to identify reason for order rejection.
/// "simpleLeavesQty": 0,// depreciated
/// "leavesQty": 0, // Quantity open for further execution.
/// "simpleCumQty": 0, //
/// "cumQty": 0, // Total number of contracts filled.
/// "avgPx": 0, // Calculated average price of all fills on this order.
/// "multiLegReportingType": "string", //
/// "text": "string",// Free format text string.
/// "transactTime": "2020-09-14T02:26:43.058Z",// Time of execution/order creation
/// "timestamp": "2020-09-14T02:26:43.058Z"
/// }
#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderInfo<'a> {
    #[serde(rename = "symbol")]
    pub(crate) specific_currency_pair: SpecificCurrencyPair,
    #[serde(rename = "orderID")]
    pub(crate) exchange_order_id: ExchangeOrderId,
    #[serde(rename = "clOrdID")]
    pub(crate) client_order_id: ClientOrderId,
    pub(crate) price: Option<Price>,
    #[serde(rename = "avgPx")]
    pub(crate) average_fill_price: Option<Price>,
    #[serde(rename = "orderQty")]
    pub(crate) amount: Option<Amount>,
    #[serde(rename = "cumQty")]
    pub(crate) filled_amount: Option<Amount>,
    #[serde(rename = "ordStatus")]
    pub(crate) status: &'a str,
    pub(crate) side: OrderSide,
}

/// Bitmex Order Book description
/// Price and Size fields are optional
/// {
/// symbol: "string" // currency pair
/// id: 0 // unique id of order book record
/// side: "string" // order side, "Buy" or "Sell"
/// size: 0 // amount value
/// price: 0 // price value
/// }
#[derive(Deserialize, Debug)]
pub struct BitmexOrderBookInsert {
    pub symbol: SpecificCurrencyPair,
    pub id: u64,
    pub side: OrderSide,
    pub size: Amount,
    pub price: Price,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderBookDelete {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) id: u64,
    pub(crate) side: OrderSide,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderBookUpdate {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) id: u64,
    pub(crate) side: OrderSide,
    pub(crate) size: Amount,
}

/// Bitmex execution response description.
/// We use it for trades, order fills and order status changing via websocket
///{
///"execID": "string",// Unique identifier of execution message assigned by BitMEX.
///"orderID": "string",// Unique identifier for Order assigned by BitMEX.
///"clOrdID": "string", // clOrdID refers to "Client Order ID" which is an optional field that you can use to personally identify your open orders (not BitMEX-generated).  The value assigned to this field needs to be unique amongst your open orders and cannot be in use by an existing open order. If an open order exists with the same value, your request to open a new order with the same clOrdID will be rejected.  Once an order has been filled or cancelled, the value can be reused.
///"clOrdLinkID": "string", // Permits order originators to tie together groups of orders in which trades resulting from orders are associated for a specific purpose, for example, the calculation of average execution price for a customer or to associate lists submitted to a broker as waves of a larger program trade.
///"account": 0,// Unique identifier for the account.
///"symbol": "string", // Code representing the product for this execution.
///"side": "string", // The side of the execution (Buy or Sell).
///"lastQty": 0,// Executed quantity.
///"lastPx": 0, // Executed price.
///"underlyingLastPx": 0, // Legacy field (e.g. was used for calendar spread products).
///"lastMkt": "string", // Venue of the execution (always XBME = BitMEX).
///"lastLiquidityInd": "string",// Indication of whether this execution was passive or aggressive (e.g.Market Buy - "RemovedLiquidity”)
///"simpleOrderQty": 0, // orderQty in terms of the underlying symbol.
///"orderQty": 0, // Quantity of the order related to this execution.
///"price": 0,// Price of the order related to this execution.
///"displayQty": 0, // Quantity of the order related to this execution (non-hidden).
///"stopPx": 0, // Stop Price (triggering price) of the order related to this execution.
///"pegOffsetValue": 0, // Offset for the pegged order related to this execution.
///"pegPriceType": "string", // Type of the peg price of the order related to this execution.
///"currency": "string", // The quote currency of the symbol (symbol.quoteCurrency)
///"settlCurrency": "string", // The settlement currency of the symbol (symbol.settlCurrency).
///"execType": "string", // The type of the execution (e.g. Trade, Settlement).
///"ordType": "string", // The type of the order related to this execution (e.g. Limit, Market).
///"timeInForce": "string", // Timeframe for executing the order related to this execution.  Absence of this field is interpreted as DAY. (i.e. "ImmediateOrCancel",)
///"execInst": "string", // Execution instructions on the order related to this execution (e.g. ParticipateDoNotInitiate, Close)
///"contingencyType": "string", // Legacy field (e.g. was used for contingent order types).
///"exDestination": "string", // Venue instruction on order related to this execution.
///"ordStatus": "string", // Status of the order related to this execution (Possible values: New, Filled, PartiallyFilled, Canceled, Rejected)
///"triggered": "string", // Indication of whether a stop order is triggered or not (e.g "StopOrderTriggered")
///"workingIndicator": true, // Indication of whether the order is live in the order book (e.g. Untriggered stop orders, or terminal state orders would be false)
///"ordRejReason": "string",// For orders where ordStatus="Rejected" this column will give reason for rejections.
///"simpleLeavesQty": 0, // leavesQty in units of the underlying
///"leavesQty": 0, // The quantity remaining to be filled in the order associated with this execution.  leavesQty = orderQty - cumQty
///"simpleCumQty": 0, // cumQty in units of the underlying
///"cumQty": 0, // The quantity filled in this execution for the order associated with this execution.  The general rule is: OrderQty = CumQty + LeavesQty.
///"avgPx": 0, // Average filled price of the order associated with this execution
///"commission": 0, //    For execType=Trade, this is the Maker/Taker fee rate. For execType=Funding this is the funding rate. For execType=Settlement, this is the Settlement Fee rate
///"tradePublishIndicator": "string", // Whether this execution resulted in a publically published trade (e.g. "DoNotPublishTrade", "PublishTrade")
///"multiLegReportingType": "string", // Legacy field (e.g. was used for calendar spread products)
///"text": "string", // Audit field containing descriptions of changes to the order related to this execution (e.g. "Submission from www.bitmex.com")
///"trdMatchID": "string", // Unique identifier for all executions in a match event
///"execCost": 0,  // Cost of the execution in terms of the settlCurrency (round(1e8/price) * number of contracts).
///"execComm": 0, // Calculated commission for this execution based on commission (field)
///"homeNotional": 0, // The value of the execution in terms of the underlying
///"foreignNotional": 0, // The value of the execution in terms of the quoteCurrency
///"transactTime": "2020-09-14T02:26:42.839Z", // Time the execution logically occured - the time priority, i.e. when it entered the order book
///"timestamp": "2020-09-14T02:26:42.839Z" // Time the execution was actually processed
///}
#[derive(Deserialize, Debug)]
pub(crate) struct BitmexTradePayload {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) side: OrderSide,
    pub(crate) size: Amount,
    pub(crate) price: Price,
    #[serde(rename = "trdMatchID")]
    pub(crate) trade_id: String,
    #[serde(deserialize_with = "deserialize_datetime")]
    pub(crate) timestamp: DateTime,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderStatus<'a> {
    #[serde(rename = "execInst")]
    pub(crate) instruction: &'a str,
    #[serde(rename = "clOrdID")]
    pub(crate) client_order_id: ClientOrderId,
    #[serde(rename = "orderID")]
    pub(crate) exchange_order_id: ExchangeOrderId,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderFillTrade<'a> {
    #[serde(rename = "text")]
    pub(crate) details: String,
    #[serde(rename = "execID")]
    pub(crate) trade_id: String,
    #[serde(rename = "clOrdID")]
    pub(crate) client_order_id: ClientOrderId,
    #[serde(rename = "orderID")]
    pub(crate) exchange_order_id: ExchangeOrderId,
    #[serde(rename = "lastPx")]
    pub(crate) fill_price: Price,
    #[serde(rename = "lastQty")]
    pub(crate) fill_amount: Amount,
    #[serde(rename = "cumQty")]
    pub(crate) total_filled_amount: Amount,
    #[serde(rename = "orderQty")]
    pub(crate) amount: Amount,
    #[serde(deserialize_with = "deserialize_datetime")]
    pub(crate) timestamp: DateTime,
    pub(crate) side: OrderSide,
    pub(crate) symbol: SpecificCurrencyPair,
    #[serde(rename = "settlCurrency")]
    pub(crate) currency: &'a str,
    #[serde(rename = "commission")]
    pub(crate) commission_rate: Decimal,
    #[serde(rename = "execComm")]
    pub(crate) commission_amount: Decimal,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderFillDummy {}

/// Bitmex Balance response description
///{
///"account": 0,
///"currency": "string",
///"riskLimit": 0,
///"prevState": "string",
///"state": "string",
///"action": "string",
///"amount": 0,
///"pendingCredit": 0,
///"pendingDebit": 0,
///"confirmedDebit": 0,
///"prevRealisedPnl": 0,
///"prevUnrealisedPnl": 0,
///"grossComm": 0,
///"grossOpenCost": 0,
///"grossOpenPremium": 0,
///"grossExecCost": 0,
///"grossMarkValue": 0,
///"riskValue": 0,
///"taxableMargin": 0,
///"initMargin": 0,
///"maintMargin": 0,
///"sessionMargin": 0,
///"targetExcessMargin": 0,
///"varMargin": 0,
///"realisedPnl": 0,
///"unrealisedPnl": 0,
///"indicativeTax": 0,
///"unrealisedProfit": 0,
///"syntheticMargin": 0,
///"walletBalance": 0,
///"marginBalance": 0,
///"marginBalancePcnt": 0,
///"marginLeverage": 0,
///"marginUsedPcnt": 0,
///"excessMargin": 0,
///"excessMarginPcnt": 0,
///"availableMargin": 0,
///"withdrawableMargin": 0,
///"makerFeeDiscount": 0,
///"takerFeeDiscount": 0,
///"timestamp": "2022-10-05T12:32:08.964Z"
///}
#[derive(Deserialize, Debug)]
pub(crate) struct BitmexBalanceInfo<'a> {
    pub(crate) currency: &'a str,
    #[serde(rename = "availableMargin")]
    pub(crate) balance: Decimal,
}

/// Bitmex position response description
///{
///"account": 0,    // Your unique account ID
///"symbol": "string",  // The contract for this position
///"currency": "string",    // The margin currency for this position
///"underlying": "string",  // Meta data of the symbol
///"quoteCurrency": "string",   // Meta data of the symbol, All prices are in the quoteCurrency
///"commission": 0, // The maximum of the maker, taker, and settlement fee
///"initMarginReq": 0,  // The initial margin requirement. This will be at least the symbol's default initial maintenance margin, but can be higher if you choose lower leverage.
///"maintMarginReq": 0, // The maintenance margin requirement. This will be at least the symbol's default maintenance maintenance margin, but can be higher if you choose a higher risk limit
///"riskLimit": 0,  // This is a function of your maintMarginReq
///"leverage": 0,   // initMarginReq.
///"crossMargin": true, // True/false depending on whether you set cross margin on this position.
///"deleveragePercentile": 0,   // Indicates where your position is in the ADL queue.
///"rebalancedPnl": 0,  // The value of realised PNL that has transferred to your wallet for this position
///"prevRealisedPnl": 0,    // The value of realised PNL that has transferred to your wallet for this position since the position was closed.
///"prevUnrealisedPnl": 0,
///"prevClosePrice": 0,
///"openingTimestamp": "2022-10-05T12:32:08.647Z",
///"openingQty": 0,
///"openingCost": 0,    
///"openingComm": 0,
///"openOrderBuyQty": 0,
///"openOrderBuyCost": 0,
///"openOrderBuyPremium": 0,
///"openOrderSellQty": 0,
///"openOrderSellCost": 0,
///"openOrderSellPremium": 0,
///"execBuyQty": 0,
///"execBuyCost": 0,
///"execSellQty": 0,
///"execSellCost": 0,
///"execQty": 0,
///"execCost": 0,
///"execComm": 0,
///"currentTimestamp": "2022-10-05T12:32:08.647Z",
///"currentQty": 0, // The current position amount in contracts
///"currentCost": 0,    // The current cost of the position in the settlement currency of the symbol (currency)
///"currentComm": 0,    // The current commission of the position in the settlement currency of the symbol (currency)
///"realisedCost": 0,   // The realised cost of this position calculated with regard to average cost accounting
///"unrealisedCost": 0, // currentCost - realisedCost
///"grossOpenCost": 0,  // The absolute value of your open orders for this symbol.
///"grossOpenPremium": 0,   // The amount your bidding above the mark price in the settlement currency of the symbol (currency).
///"grossExecCost": 0,
///"isOpen": true,
///"markPrice": 0,  // The mark price of the symbol in quoteCurrency.
///"markValue": 0,  // The currentQty at the mark price in the settlement currency of the symbol (currency).
///"riskValue": 0,
///"homeNotional": 0,   // Value of position in units of underlying
///"foreignNotional": 0,    // Value of position in units of quoteCurrency
///"posState": "string",
///"posCost": 0,
///"posCost2": 0,
///"posCross": 0,
///"posInit": 0,
///"posComm": 0,
///"posLoss": 0,
///"posMargin": 0,
///"posMaint": 0,
///"posAllowance": 0,
///"taxableMargin": 0,
///"initMargin": 0,
///"maintMargin": 0,
///"sessionMargin": 0,
///"targetExcessMargin": 0,
///"varMargin": 0,
///"realisedGrossPnl": 0,
///"realisedTax": 0,
///"realisedPnl": 0,    // The negative of realisedCost
///"unrealisedGrossPnl": 0, // markValue - unrealisedCost
///"longBankrupt": 0,
///"shortBankrupt": 0,
///"taxBase": 0,
///"indicativeTaxRate": 0,
///"indicativeTax": 0,
///"unrealisedTax": 0,
///"unrealisedPnl": 0,  // unrealisedGrossPnl
///"unrealisedPnlPcnt": 0,
///"unrealisedRoePcnt": 0,
///"simpleQty": 0,
///"simpleCost": 0,
///"simpleValue": 0,
///"simplePnl": 0,
///"simplePnlPcnt": 0,
///"avgCostPrice": 0,
///"avgEntryPrice": 0,
///"breakEvenPrice": 0,
///"marginCallPrice": 0,
///"liquidationPrice": 0,   // Once markPrice reaches this price, this position will be liquidated
///"bankruptPrice": 0,  // Once markPrice reaches this price, this position will have no equity
///"timestamp": "2022-10-05T12:32:08.647Z",
///"lastPrice": 0,
///"lastValue": 0
///}
#[derive(Deserialize, Debug)]
pub(crate) struct PositionPayload {
    pub(crate) symbol: SpecificCurrencyPair,
    #[serde(rename = "currentQty")]
    pub(crate) amount: Decimal,
    #[serde(rename = "avgEntryPrice")]
    pub(crate) average_entry_price: Option<Price>,
    #[serde(rename = "liquidationPrice")]
    pub(crate) liquidation_price: Option<Price>,
    pub(crate) leverage: Decimal,
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime, D::Error>
where
    D: Deserializer<'de>,
{
    struct DateTimeVisitor;

    impl<'de> de::Visitor<'de> for DateTimeVisitor {
        type Value = DateTime;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string containing json data")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let parsed = chrono::DateTime::parse_from_rfc3339(v).map_err(E::custom)?;
            Ok(DateTime::from(parsed))
        }
    }

    deserializer.deserialize_any(DateTimeVisitor)
}
