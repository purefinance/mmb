import React from "react";
import {Indicators} from "../../components/LiquidityNow";
import Body from "./body";
import Spinner from "../../controls/Spinner";
import {Container} from "react-bootstrap";

class LiquidityNow extends React.Component {
    async componentDidMount() {
        await this.updateSubscription();
    }

    async componentDidUpdate() {
        await this.updateSubscription();
    }

    async componentWillUnmount() {
        await this.props.ws.unsubscribeLiquidity();
    }

    async updateSubscription() {
        const {exchange, ws} = this.props;
        await ws.updateLiquiditySubscription(exchange.state.exchangeName, exchange.state.currencyCodePair);
    }

    render() {
        const {
            state: {orderState, currencyIndicators},
        } = this.props.ws;
        const {
            state: {symbol},
        } = this.props.exchange;

        return (
            <Container className="base-container">
                {this.props.ws.state.isConnected ? (
                    <React.Fragment>
                        {this.props.needIndicators && (
                            <Indicators
                                orderState={orderState}
                                currencyIndicators={currencyIndicators}
                                symbol={symbol}
                            />
                        )}
                        <Body exchange={this.props.exchange} ws={this.props.ws} />
                    </React.Fragment>
                ) : (
                    <Spinner />
                )}
            </Container>
        );
    }
}

export default LiquidityNow;
