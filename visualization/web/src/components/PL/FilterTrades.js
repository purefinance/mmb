import React from "react";
import Trades from "../Trades/Trades";
import {Row} from "react-bootstrap";

class FilterTrades extends React.Component {
    constructor(prop) {
        super(prop);
        this.state = {
            filterStrategies: [],
            filterExchanges: [],
            filterSymbols: [],
        };
    }

    render() {
        const {transactions} = this.props;

        return (
            <React.Fragment>
                <Row className="justify-content-md-center">
                    <strong className="topic">Trades</strong>
                </Row>
                <Row className="tradesRow">
                    <Trades isVirtualized={true} dashboard transactions={transactions} exchange={this.props.exchange} />
                </Row>
            </React.Fragment>
        );
    }
}

export default FilterTrades;
