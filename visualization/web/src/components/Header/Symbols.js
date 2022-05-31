import React from "react";
import {withRouter} from "react-router-dom";
import Dropdown from "../../controls/Dropdown/Dropdown";
import utils from "../../utils";

class Symbols extends React.Component {
    render() {
        const {interval, exchangeName, symbols, currencyCodePair} = this.props.exchange.state;
        const {isNeedInterval, isNeedExchange, isNeedCurrencyCodePair} = this.props;

        const currencies = [];
        if (symbols && symbols.length) {
            if (this.props.needAll)
                currencies.push(
                    <div
                        key={0}
                        className="dropdown-text-inner currency"
                        onClick={() =>
                            utils.pushNewLinkToHistory(this.props.history, `${this.props.path}/${exchangeName}/all`)
                        }>
                        All
                    </div>,
                );
            symbols.forEach((symbol, index) => {
                let newPath = `${this.props.path}`;
                newPath += isNeedInterval ? `/${interval}` : "";
                newPath += isNeedExchange ? `/${exchangeName}` : "";
                newPath += isNeedCurrencyCodePair ? `/${utils.urlCodePair(symbol.currencyCodePair)}` : "";

                currencies.push(
                    <div
                        key={index + 1}
                        className="dropdown-text-inner currency"
                        onClick={() => utils.pushNewLinkToHistory(this.props.history, newPath)}>
                        {symbol.currencyCodePair}
                    </div>,
                );
            });
        }
        return (
            <Dropdown
                id="SymbolsDropdown"
                headerText="currency-text"
                value={currencyCodePair === "all" ? "All symbols" : currencyCodePair}
                iconClassName="pair">
                {currencies}
            </Dropdown>
        );
    }
}

export default withRouter(Symbols);
