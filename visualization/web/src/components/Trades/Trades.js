import React from "react";
import TradeRow from "./TradeRow";
import "./Trades.css";
import CryptolpAxios from "../../cryptolpaxios";
import utils from "./utils";
import LaddaButton from "react-ladda/dist/LaddaButton";
import {ZOOM_IN} from "react-ladda/dist/constants";
import {Col, Row} from "react-bootstrap";
import {VariableSizeList as List} from "react-window";

class Trades extends React.Component {
    listOfTradesRef = React.createRef();

    constructor(props) {
        super(props);
        this.state = {
            selectedIds: {},
            cantDownloadMoreTrades: false,
            isLoading: false,
            historyTrades: [],
        };

        this.setTransactionId = this.setTransactionId.bind(this);
        this.loadMore = this.loadMore.bind(this);
    }

    loadMore = async () => {
        await this.setState({isLoading: true});

        const {longStrategyNames, exchangeName, currencyCodePair} = this.props.exchange.state;
        let historyTrades = this.state.historyTrades;

        const allTransactions = utils.concatTrades(historyTrades, this.props.transactions);
        const response = await CryptolpAxios.getTrades(
            longStrategyNames,
            exchangeName,
            currencyCodePair,
            allTransactions.length,
            100,
        );

        if (response.data) {
            const transactions = response.data.transactions;

            if (transactions.length) {
                const newHistoryTrades = utils.concatTrades(historyTrades, transactions);
                historyTrades = newHistoryTrades;
            }

            await this.setState({
                historyTrades: historyTrades,
                cantDownloadMoreTrades: transactions.length === 0,
                isLoading: false,
            });
        }
    };

    setTransactionId(index, id) {
        const selectedIds = this.state.selectedIds;

        if (selectedIds[id] !== undefined) {
            selectedIds[id] = !selectedIds[id];
        } else {
            selectedIds[id] = true;
        }

        this.setState({selectedIds: selectedIds});

        if (this.listOfTradesRef.current) {
            this.listOfTradesRef.current.resetAfterIndex(index);
        }
    }

    render() {
        const {dashboard, transactions, isVirtualized} = this.props;

        const allTransactions = utils.concatTrades(this.state.historyTrades, transactions);

        const lengthOfList = allTransactions.length + (dashboard && !this.state.cantDownloadMoreTrades ? 1 : 0);

        const getItemSize = (index) => {
            const transaction = allTransactions[index];

            if (transaction && this.state.selectedIds[transaction.id]) {
                const countOfTrades = transaction.trades.length;
                return 30 + 19 + countOfTrades * 23 + 3; // transaction row + second header + each trade with padding
            } else if (index === allTransactions.length) {
                return 45; // ladda button
            } else {
                return 30; // transaction row
            }
        };

        return (
            <React.Fragment>
                <Col id="trades" className={`base-col base-background`}>
                    <Row className="base-row">
                        {dashboard && (
                            <React.Fragment>
                                <Col className="base-col title_block_general title_regulare_instant">Exchange</Col>
                                <Col className="base-col title_block_general title_regulare_instant">Currency</Col>
                            </React.Fragment>
                        )}
                        <Col
                            xs={dashboard ? 2 : 4}
                            className="base-col title_block_general title_regulare_instant date_time">
                            Date &amp; Time
                        </Col>
                        <Col className="base-col title_block_general title_regulare_instant">Side</Col>
                        <Col className="base-col title_block_general title_regulare_instant">Price</Col>
                        <Col className="base-col title_block_general title_regulare_instant">Amount</Col>
                        <Col className="base-col title_block_general title_regulare_instant">Hedged</Col>
                        <Col className="base-col title_block_general title_regulare_instant">PL</Col>
                        {dashboard && (
                            <Col
                                xs={dashboard ? 2 : 1}
                                className="base-col title_block_general title_regulare_instant strategy">
                                Strategy
                            </Col>
                        )}
                        <Col className="base-col title_block_general title_regulare_instant">Status</Col>
                    </Row>
                    <Col className={`base-col gridLine ${dashboard ? "dashboard" : ""}`} />

                    {isVirtualized === true ? (
                        <List
                            ref={this.listOfTradesRef}
                            height={633}
                            itemCount={lengthOfList}
                            itemSize={getItemSize}
                            width={"100%"}>
                            {({index, style}) =>
                                index < allTransactions.length ? (
                                    this.getTradeRow(allTransactions[index], dashboard, index, style)
                                ) : (
                                    <LaddaButton
                                        style={style}
                                        loading={this.state.isLoading}
                                        onClick={this.loadMore}
                                        data-color="#f88710"
                                        data-style={ZOOM_IN}
                                        data-spinner-size={30}
                                        data-spinner-color="#ffffff"
                                        data-spinner-lines={10}
                                        className="custom-LadaButton loadMore">
                                        More...
                                    </LaddaButton>
                                )
                            }
                        </List>
                    ) : (
                        allTransactions.map((transaction, index) => {
                            return this.getTradeRow(transaction, dashboard, index);
                        })
                    )}
                </Col>
            </React.Fragment>
        );
    }

    getTradeRow(transaction, dashboard, index, style) {
        return (
            <TradeRow
                style={style}
                setTransactionId={(e) => this.setTransactionId(index, transaction.id)}
                isSelect={this.state.selectedIds[transaction.id]}
                dashboard={dashboard}
                key={index}
                transaction={transaction}
            />
        );
    }
}

export default Trades;
