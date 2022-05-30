import React from "react";
import {withRouter} from "react-router-dom";
import Spinner from "../../controls/Spinner";
import Body from "./body";
import Indicators from "../../components/Liquidity/Indicators";
import LiquidityNow from "../LiquidityNow";
import {Container} from "react-bootstrap";

class Liquidity extends React.Component {
    async componentDidMount() {
        await this.update();
    }

    async componentDidUpdate() {
        await this.update();
    }

    async update() {
        const {exchange, liquidity} = this.props;
        const exState = exchange.state;
        const liqState = liquidity.state;

        if (
            exState.interval !== liqState.interval ||
            exState.exchangeName !== liqState.exchangeName ||
            exState.currencyCodePair !== liqState.currencyCodePair
        ) {
            //Temporarily unused
            // await liquidity.updateSelection(exState.exchangeName, exState.currencyPair, exState.currencyCodePair, exState.interval);
        }
    }

    async componentWillUnmount() {
        await this.props.liquidity.clearData();
    }

    render() {
        const {
            state: {symbol, interval},
        } = this.props.exchange;
        if (interval === "now") {
            return <LiquidityNow needIndicators exchange={this.props.exchange} ws={this.props.ws} />;
        }

        const {
            state: {currencyPair, exchangeName, liquidityIndicators, preprocessedOrderBook},
        } = this.props.liquidity;

        return (
            <Container className="base-container">
                <Indicators liquidityIndicators={liquidityIndicators} symbol={symbol} />
                {currencyPair && exchangeName ? (
                    <Body
                        exchange={this.props.exchange}
                        liquidity={this.props.liquidity}
                        isLoading={!liquidityIndicators || !preprocessedOrderBook}
                    />
                ) : (
                    <Spinner />
                )}
            </Container>
        );
    }
}

export default withRouter(Liquidity);
