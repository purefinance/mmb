import React from "react";
import {BodyErrorBoundary} from "../../errorBoundaries";
import OrderBookAndTrades from "../../components/LiquidityNow/OrderBookAndTrades";

class Body extends React.Component {
    render() {
        const {
            state: {orderState},
        } = this.props.ws;
        const {
            state: {symbol},
        } = this.props.exchange;
        return (
            <BodyErrorBoundary>
                <OrderBookAndTrades orderState={orderState} symbol={symbol} />
            </BodyErrorBoundary>
        );
    }
}

export default Body;
