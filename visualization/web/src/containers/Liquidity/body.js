import React from "react";
import LiquidityChart from "../../components/Liquidity/Liquidity";
import Spinner from "../../controls/Spinner";

import {BodyErrorBoundary} from "../../errorBoundaries";
import {Container} from "react-bootstrap";

class Body extends React.Component {
    render() {
        let {
            state: {liquidityIndicators, preprocessedOrderBook},
        } = this.props.liquidity;
        let {isLoading} = this.props;
        return !isLoading ? (
            <BodyErrorBoundary>
                <Container className="base-background base-container">
                    <LiquidityChart
                        liquidityIndicators={liquidityIndicators}
                        preprocessedOrderBook={preprocessedOrderBook}
                        symbol={this.props.exchange.state.symbol}
                    />
                </Container>
            </BodyErrorBoundary>
        ) : (
            <Spinner />
        );
    }
}

export default Body;
