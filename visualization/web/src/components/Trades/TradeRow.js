import React from "react";
import utils from "../../utils";
import {Row, Col} from "react-bootstrap";

const getTradeFillRows = (transactionSide, trades) => {
    return trades.map((trade, index) => {
        const isBuy = transactionSide === 1;
        const isLong = trade.direction === 0 ? isBuy : !isBuy; // 0 -> Target, 1 -> Hedged
        return (
            <Row className={`base-row bottom-line row-${isLong ? "long" : "short"}`} key={index}>
                <Col className="base-col center" title={trade.exchangeOrderId}>
                    {trade.exchangeOrderId ? trade.exchangeOrderId.slice(-10) : "--"}
                </Col>
                <Col className="base-col center">{trade.exchangeName}</Col>
                <Col className="base-col center">{trade.price}</Col>
                <Col className="base-col center">{trade.amount}</Col>
                <Col className="base-col center" title={trade.dateTime}>
                    {utils.toLocalTime(trade.dateTime)}
                </Col>
            </Row>
        );
    });
};

const getAllTradeFillRows = (transaction) => {
    const trades = [];

    trades.push(...getTradeFillRows(transaction.side, transaction.trades));

    return trades;
};

function TradeRow(props) {
    const arrow = (
        <i
            className={`fas fa-angle-down icon-arrow-trades ${props.isSelect ? "select" : ""} cursor`}
            onClick={props.setTransactionId}
        />
    );

    const trades = getAllTradeFillRows(props.transaction);
    const isBuy = props.transaction.side === 1;

    return (
        <React.Fragment>
            <Row style={props.style} className="base-row">
                {props.dashboard && (
                    <React.Fragment>
                        <Col className="base-col exchange_market_item">
                            {arrow}
                            {props.transaction.exchangeName}
                        </Col>
                        <Col className="base-col currency_pair_item">{props.transaction.currencyCodePair}</Col>
                    </React.Fragment>
                )}
                <Col
                    xs={props.dashboard ? 2 : 4}
                    className={`base-col date_time_item`}
                    title={props.transaction.dateTime}>
                    {!props.dashboard && arrow}
                    {utils.toLocalDateTime(props.transaction.dateTime)}
                </Col>
                <Col className={`base-col side_item ${isBuy ? "buy" : "sell"}-color`}>{isBuy ? "Buy" : "Sell"}</Col>
                <Col className="base-col price_item">{props.transaction.price}</Col>
                <Col className="base-col amount_item">{props.transaction.amount}</Col>
                <Col className={`base-col hedged_item ${props.transaction.hedged < 1 ? "orange-hedged" : ""}`}>
                    {props.transaction.hedged === null ? "--" : (props.transaction.hedged * 100).toFixed(1) + "%"}
                </Col>
                <Col className="base-col ls_item">
                    {props.transaction.profitLossPct === null ? "--" : props.transaction.profitLossPct.toFixed(3) + "%"}
                </Col>
                {props.dashboard && (
                    <Col xs={props.dashboard ? 2 : 1} className="base-col currency_pair_item">
                        {props.transaction.strategyName}
                    </Col>
                )}
                <Col className="base-col amount_item">{props.transaction.status}</Col>
                <Row className={`base-row trade_info ${props.isSelect ? "select" : ""}`}>
                    <Row className="base-row">
                        <Col className="base-col title-text">OrderId</Col>
                        <Col className="base-col center title-text">Exchange</Col>
                        <Col className="base-col center title-text">Price</Col>
                        <Col className="base-col center title-text">Amount</Col>
                        <Col className="base-col center title-text">DateTime</Col>
                    </Row>
                    {trades}
                </Row>
            </Row>
        </React.Fragment>
    );
}

export default TradeRow;
