import {Container} from "unstated";
import CryptolpAxios from "../cryptolpaxios";
import {removeDuplicates} from "../controls/functions";

class ExchangeContainer extends Container {
    constructor(props) {
        super(props);
        this.state = {
            currencyCodePair: "",
            currencyPair: "",
            exchangeName: "",
            strategyName: "All",
            exchanges: null,
            symbols: null,
            interval: "now",
            selectedSymbol: -1,
            selectedExchange: -1,
            selectedStrategy: -1,
            shortStrategyNames: null,
            longStrategyNames: null,
            symbol: null,
        };

        this.loadExchanges();

        this.updateSelectedStrategy = this.updateSelectedStrategy.bind(this);
        this.updateSelectedExchange = this.updateSelectedExchange.bind(this);
        this.updateSelectedSymbol = this.updateSelectedSymbol.bind(this);
        this.updateSelected = this.updateSelected.bind(this);
    }

    async loadExchanges() {
        // emulate loading exchanges
        // const res = await CryptolpAxios.getSupportedExchanges();

        let res = {
            supportedExchanges: [{
                symbols: [{
                    currencyCodePair: "$currencyCodePair",
                    currencyPair: "$currencyPair"
                }]
            }],
            shortStrategyNames: ["all"],
            longStrategyNames: ["all"]
        }

        await this.setState({
            exchanges: res.supportedExchanges,
            symbols: res.supportedExchanges[0].symbols,
            selectedExchange: 0,
            selectedSymbol: 0,
            shortStrategyNames: res.shortStrategyNames,
            longStrategyNames: res.longStrategyNames,
        });
        await this.updateSelected();

        this.state.currencyPair = "$currencyPair"
        this.state.currencyCodePair = "$currencyCodePair"
        this.state.exchangeName = "$exchangeName"
    }

    async updateSelected() {
        const {exchanges, symbols, selectedExchange, selectedSymbol, shortStrategyNames, selectedStrategy} = this.state;

        const exchangeName = selectedExchange === -1 ? "all" : exchanges[selectedExchange].name;
        const symbol = this.getSymbol(symbols, selectedSymbol);
        const strategyName =
            selectedStrategy === -1 || !shortStrategyNames ? "all" : shortStrategyNames[selectedStrategy];

        await this.setState({
            exchangeName: exchangeName,
            currencyCodePair: symbol.currencyCodePair,
            currencyPair: symbol.currencyPair,
            symbol: symbol.symbol,
            strategyName: strategyName,
        });
    }

    async updateSelectedStrategy(index) {
        const {shortStrategyNames} = this.props.state;

        const strategyName = index === -1 || !shortStrategyNames ? "all" : shortStrategyNames[index];

        await this.setState({
            selectedStrategy: index,
            strategyName: strategyName,
        });
        console.log("updatedSelectedStrategy " + index);
    }

    async updateSelectedExchange(index) {
        const {exchanges} = this.state;

        let symbols = [];
        if (index === -1) {
            exchanges.forEach((ex) => symbols.push(...ex.symbols));
            symbols = removeDuplicates(symbols, "currencyCodePair");
        } else symbols = exchanges[index].symbols;

        const exchangeName = index === -1 ? "all" : exchanges[index].name;
        const selectedSymbol = index === -1 ? -1 : 0;
        const symbol = this.getSymbol(symbols, selectedSymbol);

        await this.setState({
            selectedExchange: index,
            exchangeName: exchangeName,
            symbols: symbols,
            selectedSymbol: selectedSymbol,
            currencyCodePair: symbol.currencyCodePair,
            currencyPair: symbol.currencyPair,
            symbol: symbol.symbol,
        });
        console.log("updatedSelectedExchange " + index);
    }

    async updateSelectedSymbol(index) {
        const symbol = this.getSymbol(this.state.symbols, index);

        await this.setState({
            selectedSymbol: index,
            currencyCodePair: symbol.currencyCodePair,
            currencyPair: symbol.currencyPair,
            symbol: symbol.symbol,
        });
        console.log("updateSelectedSymbol " + index);
    }

    getSymbol(symbols, index) {
        const currencyCodePair = index === -1 || !symbols ? "all" : symbols[index].currencyCodePair;
        const currencyPair = index === -1 || !symbols ? "all" : symbols[index].currencyPair;
        const symbol = index === -1 || !symbols ? null : symbols[index];

        return {
            currencyCodePair,
            currencyPair,
            symbol,
        };
    }

    async setInterval(interval) {
        await this.setState({interval: interval});
    }
}

export default ExchangeContainer;
