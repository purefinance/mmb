import { Container } from "unstated"
import Sockette from "sockette"
import config from '../config.js'
import { groupBy } from "../controls/functions"
import { delay } from "q"

class WsContainer extends Container {
    constructor (props) {
        super(props)
        this.state = this.emptyState()
        this.initiateConnection()
    }

    emptyState () {
        return {
            strategyName: "",
            currencyCodePair: "$currencyCodePair",
            currencyPair: "",
            exchangeName: "$exchangeName",
            currencyIndicators: null,
            orderState: null,
            transactions: null,
            profits: null,
            volumeNow: null,
            dashboardIndicators: null,
            balances: null,
            tradeSignals: null,
            isConnected: false,
            subscribedLiquidity: "",
            subscribedVolume: "",
            subscribedPL: "",
            subscribedDashboard: false,
            subscribedBalances: false,
            subscribedTradeSignals: false
        }
    }

    async router (command, message) {
        switch (command) {
        case "UpdateOrdersState": {
            console.log("OrderState update")
            await this.updateOrderState(message)
            break
        }
        case "UpdateDashboard": {
            console.log("Dashboard update")
            await this.updateIndicators(message)
            break
        }
        case "UpdateBalances": {
            console.log("Balances update")
            await this.updateBalances(message)
            break
        }
        case "UpdatePL": {
            console.log("ProfitLoss update")
            await this.updateProfitLoss(message)
            break
        }
        case "UpdateTradeSignals": {
            console.log("TradeSignals update")
            await this.updateTradeSignals(message)
            break
        }
        case "UpdateVolume": {
            console.log("Volume update")
            await this.updateVolume(message)
            break
        }
        default: {
            console.error("Command not supported:", command, message)
        }
        }
    }

    initiateConnection () {
    // eslint-disable-next-line no-new
        new Sockette(config.baseHubURL, {
            timeout: 5e3,
            onopen: async e => {
                console.log("Websocket connected!", e)
                await this.setState({ isConnected: true })
            },
            onmessage: async e => {
                console.log("Websocket received data:", e)
                const slices = e.data.split("|")
                await this.router(slices[0], JSON.parse(slices[1]))
            },
            onreconnect: async e => {
                console.log("Websocket reconnecting...", e)
                await this.setState(this.emptyState())
            },
            onmaximum: e => console.log("Stop Attempting!", e),
            onclose: async e => {
                console.log("Websocket closed!", e)
                await this.setState(this.emptyState())
            },
            onerror: e => console.log("Error:", e)
        })

        // wsConnection = new HubConnectionBuilder()
        //     .withUrl(config.baseHubURL + "Main", {accessTokenFactory: async () => CryptolpAxios.token})
        //     .withAutomaticReconnect({
        //         nextRetryDelayInMilliseconds: (retryContext) => {
        //             return Math.random() * 10000;
        //         },
        //     })
        //     .build();

    // wsConnection.on("TokenExpired", async (message) => {
    //     console.log("Authorization token expired. Please Login in the System.");
    //
    //     wsConnection.stop();
    //
    //     localStorage.removeItem("auth_token");
    //     localStorage.removeItem("auth_expiration");
    //     localStorage.removeItem("auth_role");
    //     window.location.href = "/login";
    // });
    }

    async updateVolume (data) {
        if (this.state.subscribedVolume) {
            await this.setState({ volumeNow: data })
        }
    }

    async updateIndicators (data) {
        if (this.state.subscribedDashboard) {
            await this.setState({ dashboardIndicators: data.pairIndicators })
        }
    }

    async updateBalances (data) {
        if (this.state.subscribedBalances) {
            const grouped = groupBy(data.balances, (d) => d.currencyCode)
            // calculate total per currency
            Object.keys(grouped).forEach((curr) => {
                const sum = grouped[curr].exchanges.reduce((a, b) => a + b.value, 0)
                grouped[curr].total = sum
            })
            // grouping by currency
            await this.setState({ balances: grouped })
        }
    }

    async updateProfitLoss (data) {
        if (this.state.subscribedPL) {
            await this.setState({
                transactions: data.transactions,
                profits: data.profits
            })
        }
    }

    async updateTradeSignals (data) {
        if (this.state.subscribedTradeSignals) {
            await this.setState({ tradeSignals: data.signals })
        }
    }

    async updateOrderState (data) {
        const { exchangeName, currencyCodePair } = this.state
        const { ordersStateAndTransactions, indicators } = data
        if (
            this.state.subscribedLiquidity &&
      exchangeName === data.ordersStateAndTransactions.exchangeName &&
      currencyCodePair === data.ordersStateAndTransactions.currencyCodePair
        ) {
            await this.setState({ orderState: ordersStateAndTransactions })
            await this.setState({ currencyIndicators: indicators })
        }
    }

    static isInvokeInProgress = false

    async oneTimeInvoke (newState, method, ...props) {
        if (!this.isInvokeInProgress && this.state.isConnected) {
            this.isInvokeInProgress = true
            // disable subscribe on server
            // await wsConnection.invoke(method, ...props)
            await this.setState(newState)
            this.isInvokeInProgress = false
            console.log(method + " completed")
        }
    }

    async retryInvoke (bool, checkField, ...props) {
        while (bool ? this.state[checkField] : !this.state[checkField]) {
            await this.oneTimeInvoke(...props)
            await delay(100)
        }
    }

    async subscribeDashboard () {
        await this.retryInvoke(false, "subscribedDashboard", { subscribedDashboard: true }, "SubscribeDashboard")
    }

    async unsubscribeDashboard () {
        await this.retryInvoke(
            true,
            "subscribedDashboard",
            { subscribedDashboard: false, dashboardIndicators: null },
            "UnsubscribeDashboard"
        )
    }

    async subscribeBalances () {
        await this.retryInvoke(false, "subscribedBalances", { subscribedBalances: true }, "SubscribeBalances")
    }

    async unsubscribeBalances () {
        await this.retryInvoke(
            true,
            "subscribedBalances",
            { subscribedBalances: false, balances: null },
            "UnsubscribeBalances"
        )
    }

    async subscribeTradeSignals () {
        await this.retryInvoke(
            false,
            "subscribedTradeSignals",
            { subscribedTradeSignals: true },
            "SubscribeTradeSignals"
        )
    }

    async unsubscribeTradeSignals () {
        await this.retryInvoke(
            true,
            "subscribedTradeSignals",
            { subscribedTradeSignals: false, tradeSignals: null },
            "UnsubscribeTradeSignals"
        )
    }

    async unsubscribePL () {
        if (this.state.subscribedPL) {
            const currentCodes = this.state.subscribedPL.split("|")
            await this.retryInvoke(
                true,
                "subscribedPL",
                { subscribedPL: "", transactions: null, profits: null },
                "UnsubscribePL",
                currentCodes[0],
                currentCodes[1],
                currentCodes[2],
                100
            )
        }
    }

    getSubscribeString (...props) {
        let res = ""

        let isStrings = true
        props.forEach((prop) => {
            if (typeof prop !== "string") {
                isStrings = false
            }
        })

        if (!isStrings) return res

        let isNotEmpty = true
        props.forEach((prop) => {
            if (prop.length === 0) {
                isNotEmpty = false
            }
        })

        if (!isNotEmpty) return res

        props.forEach((prop, index) => {
            res += prop + (index === props.length - 1 ? "" : "|")
        })

        return res
    }

    async updatePLSubscription (strategyName, exchangeName, currencyCodePair) {
        const newSubscribed = this.getSubscribeString(strategyName, currencyCodePair, exchangeName)

        if (this.state.subscribedPL !== newSubscribed) {
            await this.unsubscribePL()

            if (newSubscribed && newSubscribed.length !== 0) {
                await this.retryInvoke(
                    false,
                    "subscribedPL",
                    {
                        subscribedPL: newSubscribed,
                        exchangeName,
                        currencyCodePair,
                        strategyName
                    },
                    "SubscribePL",
                    strategyName,
                    currencyCodePair,
                    exchangeName,
                    100
                )
            }
        }
    }

    async unsubscribeLiquidity () {
        if (this.state.subscribedLiquidity) {
            const currentCodes = this.state.subscribedLiquidity.split("|")
            await this.retryInvoke(
                true,
                "subscribedLiquidity",
                {
                    subscribedLiquidity: "",
                    orderState: null
                },
                "UnsubscribeLiquidity",
                currentCodes[0],
                currentCodes[1]
            )
        }
    }

    async updateLiquiditySubscription (exchangeName, currencyCodePair) {
        const newSubscribed = this.getSubscribeString(exchangeName, currencyCodePair)

        if (this.state.subscribedLiquidity !== newSubscribed) {
            await this.unsubscribeLiquidity()

            if (newSubscribed) {
                await this.retryInvoke(
                    false,
                    "subscribedLiquidity",
                    {
                        subscribedLiquidity: newSubscribed,
                        exchangeName,
                        currencyCodePair
                    },
                    "SubscribeLiquidity",
                    exchangeName,
                    currencyCodePair
                )
            }
        }
    }

    async unsubscribeVolume () {
        if (this.state.subscribedVolume) {
            const currentCodes = this.state.subscribedVolume.split("|")
            await this.retryInvoke(
                true,
                "subscribedVolume",
                {
                    subscribedVolume: "",
                    volumeNow: null
                },
                "UnsubscribeVolume",
                currentCodes[0],
                currentCodes[1]
            )
        }
    }

    async updateVolumeSubscription (exchangeName, currencyCodePair) {
        const newSubscribed = this.getSubscribeString(exchangeName, currencyCodePair)

        if (this.state.subscribedVolume !== newSubscribed) {
            await this.unsubscribeVolume()

            if (newSubscribed) {
                await this.retryInvoke(
                    false,
                    "subscribedVolume",
                    {
                        subscribedVolume: newSubscribed,
                        exchangeName,
                        currencyCodePair
                    },
                    "SubscribeVolume",
                    currencyCodePair,
                    exchangeName
                )
            }
        }
    }

    async updateState (exchangeName, currencyPair, currencyCodePair) {
        await this.setState({
            exchangeName,
            currencyPair,
            currencyCodePair
        })
    }
}

export default WsContainer
