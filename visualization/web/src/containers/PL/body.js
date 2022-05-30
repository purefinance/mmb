import React from "react";
import {BodyErrorBoundary} from "../../errorBoundaries";
import {Row, Col} from "react-bootstrap";
import Spinner from "../../controls/Spinner";
import {PLIndicator, FilterTrades} from "../../components/PL";
import "./body.css";

function getProfit(profit, profitType) {
    return profit ? profit[profitType] : null;
}

class Body extends React.Component {
    render() {
        const {
            state: {transactions, profits},
        } = this.props.ws;
        const rawProfit = profits ? profits.rawProfit : null;
        const profitOverMarket = profits ? profits.profitOverMarket : null;

        const dayProfit = "dayProfit";
        const weekProfit = "weekProfit";
        const monthProfit = "monthProfit";

        return (
            <BodyErrorBoundary>
                {this.props.ws.state.isConnected && profits && transactions ? (
                    <Col>
                        <Row className="base-container base-row">
                            <PLIndicator
                                raw={getProfit(rawProfit, dayProfit)}
                                overMarket={getProfit(profitOverMarket, dayProfit)}
                                title="Day"
                            />
                            <PLIndicator
                                raw={getProfit(rawProfit, weekProfit)}
                                overMarket={getProfit(profitOverMarket, weekProfit)}
                                title="Week"
                            />
                            <PLIndicator
                                raw={getProfit(rawProfit, monthProfit)}
                                overMarket={getProfit(profitOverMarket, monthProfit)}
                                title="Month"
                            />
                        </Row>
                        <Col className="tradesCol">
                            <FilterTrades transactions={transactions} exchange={this.props.exchange} />
                        </Col>
                    </Col>
                ) : (
                    <Spinner />
                )}
            </BodyErrorBoundary>
        );
    }
}

export default Body;
