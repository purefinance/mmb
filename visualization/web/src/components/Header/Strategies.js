import React from "react";
import {withRouter} from "react-router-dom";
import Dropdown from "../../controls/Dropdown/Dropdown";
import utils from "../../utils";

class Strategies extends React.Component {
    render() {
        const {shortStrategyNames, strategyName, exchangeName, currencyCodePair} = this.props.exchange.state;
        const {isNeedExchange, isNeedCurrencyCodePair, isNeedStrategy} = this.props;

        const strategyElements = [];
        if (shortStrategyNames) {
            strategyElements.push(
                <div
                    key={0}
                    className="dropdown-text-inner currency"
                    onClick={() =>
                        utils.pushNewLinkToHistory(
                            this.props.history,
                            `${this.props.path}/${exchangeName}/${utils.urlCodePair(currencyCodePair)}/all`)
                    }>
                    All strategies
                </div>,
            );
            shortStrategyNames.forEach((strategy, index) => {
                let newPath = `${this.props.path}`;
                newPath += isNeedExchange ? `/${exchangeName}` : "";
                newPath += isNeedCurrencyCodePair ? `/${utils.urlCodePair(currencyCodePair)}` : "";
                newPath += isNeedStrategy ? `/${strategy}` : "";

                strategyElements.push(
                    <div
                        key={index + 1}
                        className="dropdown-text-inner currency"
                        onClick={() => utils.pushNewLinkToHistory(this.props.history, newPath)}>
                        {strategy}
                    </div>,
                );
            });
        }

        return (
            <Dropdown
                id="SymbolsDropdown"
                headerText="currency-text"
                value={strategyName === "All" ? "All strategies" : strategyName}
                iconClassName="pair">
                {strategyElements}
            </Dropdown>
        );
    }
}

export default withRouter(Strategies);
