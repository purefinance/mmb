import React from "react";
import {BodyErrorBoundary} from "../../errorBoundaries";
import {Container} from "react-bootstrap";
import Spinner from "../../controls/Spinner";
import {Alert, Table} from "react-bootstrap";
import utils from "../../utils";

class TradeSignals extends React.Component {
    async componentDidMount() {
        await this.updateSubscription();
    }

    async componentDidUpdate() {
        await this.updateSubscription();
    }

    async componentWillUnmount() {
        await this.props.ws.unsubscribeTradeSignals();
    }

    async updateSubscription() {
        await this.props.ws.subscribeTradeSignals();
    }

    formatDataRow(dataRow) {
        const data = dataRow.map((x) => x.toString().substring(0, 7));
        return `[${data.join(", ")}]`;
    }

    render() {
        const {
            state: {isConnected, subscribedTradeSignals, tradeSignals},
        } = this.props.ws;

        return (
            <Container className="base-container">
                {isConnected && subscribedTradeSignals && tradeSignals ? (
                    <BodyErrorBoundary>
                        {tradeSignals.length > 0 ? (
                            <Table size="sm" className="text-center">
                                <thead>
                                    <tr>
                                        <th>Date</th>
                                        <th>Source</th>
                                        <th>Market Regime</th>
                                        <th>Imbalance Signal</th>
                                        <th>Data</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {tradeSignals.map((x, index) => {
                                        let imbalanceSignal = "";
                                        if (x.imbalanceSignal === 0) imbalanceSignal = "NoTrade";
                                        if (x.imbalanceSignal === 1) imbalanceSignal = "Buy";
                                        if (x.imbalanceSignal === 2) imbalanceSignal = "Sell";
                                        if (x.imbalanceSignal === 3) imbalanceSignal = "Both";

                                        let data = {
                                            ImbalanceSignal: imbalanceSignal,
                                            Long: {
                                                Result: "N/A",
                                                PredictedRow: ["N/A"],
                                            },
                                            Short: {
                                                Result: "N/A",
                                                PredictedRow: ["N/A"],
                                            },
                                        };

                                        let isParsed = true;

                                        try {
                                            data = JSON.parse(x.payload);
                                        } catch (error) {
                                            isParsed = false;
                                        }

                                        return (
                                            <tr key={index}>
                                                <td>{utils.toLocalDateTime(x.dateTime)}</td>
                                                <td>
                                                    {x.exchangeId}:{x.currencyPair}
                                                </td>
                                                <td>{x.marketRegime}</td>
                                                <td>{data.ImbalanceSignal}</td>
                                                <td>
                                                    {isParsed && (
                                                        <Table>
                                                            <thead>
                                                                <tr>
                                                                    <th>Model</th>
                                                                    <th>Decision</th>
                                                                    <th>Prediction</th>
                                                                </tr>
                                                            </thead>
                                                            <tbody>
                                                                <tr>
                                                                    <th scope="row">Long</th>
                                                                    <td>{data.Long.Result}</td>
                                                                    <td>
                                                                        {this.formatDataRow(data.Long.PredictedRow)}
                                                                    </td>
                                                                </tr>
                                                                <tr>
                                                                    <th scope="row">Short</th>
                                                                    <td>{data.Short.Result}</td>
                                                                    <td>
                                                                        {this.formatDataRow(data.Short.PredictedRow)}
                                                                    </td>
                                                                </tr>
                                                            </tbody>
                                                        </Table>
                                                    )}
                                                    {!isParsed && x.payload}
                                                </td>
                                            </tr>
                                        );
                                    })}
                                </tbody>
                            </Table>
                        ) : (
                            <Alert variant="warning">No data</Alert>
                        )}
                    </BodyErrorBoundary>
                ) : (
                    <Spinner />
                )}
            </Container>
        );
    }
}

export default TradeSignals;
