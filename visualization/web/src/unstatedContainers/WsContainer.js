import { Container } from "unstated";
import Sockette from "sockette";
import config from "../config.js";
import { groupBy } from "../controls/functions";
import CryptolpAxios from "../cryptolpaxios";
import { toast } from "react-toastify";
import "react-toastify/dist/ReactToastify.css";

class WsContainer extends Container {
  constructor(props) {
    super(props);
    this.state = this.emptyState();
    this.initiateConnection();
  }

  emptyState() {
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
      subscribedTradeSignals: false,
      auth: false,
    };
  }

  async router(command, message) {
    switch (command) {
      case "Authorized": {
        console.log("Authorized message");
        await this.processAuthorized(message);
        break;
      }
      case "Pong": {
        console.log("Pong message");
        await this.pong();
        break;
      }
      case "UpdateOrdersState": {
        console.log("OrderState update");
        await this.updateOrderState(message);
        break;
      }
      case "UpdateDashboard": {
        console.log("Dashboard update");
        await this.updateIndicators(message);
        break;
      }
      case "UpdateBalances": {
        console.log("Balances update");
        await this.updateBalances(message);
        break;
      }
      case "UpdatePL": {
        console.log("ProfitLoss update");
        await this.updateProfitLoss(message);
        break;
      }
      case "UpdateTradeSignals": {
        console.log("TradeSignals update");
        await this.updateTradeSignals(message);
        break;
      }
      case "UpdateVolume": {
        console.log("Volume update");
        await this.updateVolume(message);
        break;
      }
      case "Error": {
        console.log("Error message received", message);
        toast.error(message.message);
        toast.clearWaitingQueue();
        break;
      }
      default: {
        console.error("Command not supported:", command, message);
      }
    }
  }

  initiateConnection() {
    // eslint-disable-next-line no-new
    this.wsConnection = new Sockette(config.baseHubURL, {
      timeout: 5e3,
      onopen: async (e) => {
        console.log("Websocket connected!", e);
        await this.setState({ isConnected: true });
        await this.auth(localStorage.getItem("auth_token"));

        clearTimeout(this.pingTimeout);
        await this.ping();
        clearTimeout(this.hbTimeout);
        await this.hb();
      },
      onmessage: async (e) => {
        console.log("Websocket received data:", e);
        const slices = e.data.split("|");
        await this.router(slices[0], JSON.parse(slices[1]));
      },
      onreconnect: async (e) => {
        console.log("Websocket reconnecting...", e);
        await this.setState(this.emptyState());
      },
      onmaximum: (e) => console.log("Stop Attempting!", e),
      onclose: async (e) => {
        this.stopPing();
        console.log("Websocket closed!", e);
        await this.setState(this.emptyState());
      },
      onerror: (e) => {
        this.stopPing();
        console.error("Error:", e);
        toast.clearWaitingQueue();
        toast.error("Connection problem");
      },
    });
  }

  async ping() {
    try {
      this.wsConnection.send(`Ping|`);
      this.pingTimeout = setTimeout(() => {
        this.ping();
      }, 1000);
    } catch (err) {
      console.log("ping error", err);
    }
  }

  async hb() {
    let currentTime = Date.now();
    if (this.lastPongTime && currentTime > this.lastPongTime + 4000) {
      console.error("Connection problem. Disconnected");
      this.stopHb();
      this.lastPongTime = null;
      await this.setState(this.emptyState());
      this.wsConnection.close();
      this.initiateConnection();
      return;
    }
    this.hbTimeout = setTimeout(() => {
      this.hb();
    }, 1000);
  }

  stopPing() {
    clearTimeout(this.pingTimeout);
  }

  stopHb() {
    clearTimeout(this.hbTimeout);
  }

  async pong() {
    this.lastPongTime = Date.now();
  }

  async auth(token) {
    await this.retryInvoke(
      false,
      "auth",
      { auth: true },
      "Auth",
      JSON.stringify({ token })
    );
  }

  async updateVolume(data) {
    if (this.state.subscribedVolume) {
      await this.setState({ volumeNow: data });
    }
  }

  async updateIndicators(data) {
    if (this.state.subscribedDashboard) {
      await this.setState({ dashboardIndicators: data.pairIndicators });
    }
  }

  async updateBalances(data) {
    if (this.state.subscribedBalances) {
      const grouped = groupBy(data.balances, (d) => d.currencyCode);
      // calculate total per currency
      Object.keys(grouped).forEach((curr) => {
        grouped[curr].total = grouped[curr].exchanges.reduce(
          (a, b) => a + b.value,
          0
        );
      });
      // grouping by currency
      await this.setState({ balances: grouped });
    }
  }

  async updateProfitLoss(data) {
    if (this.state.subscribedPL) {
      await this.setState({
        transactions: data.transactions,
        profits: data.profits,
      });
    }
  }

  async updateTradeSignals(data) {
    if (this.state.subscribedTradeSignals) {
      await this.setState({ tradeSignals: data.signals });
    }
  }

  async updateOrderState(data) {
    const { exchangeName, currencyCodePair } = this.state;
    const { ordersStateAndTransactions, indicators } = data;
    if (
      this.state.subscribedLiquidity &&
      exchangeName === data.ordersStateAndTransactions.exchangeName &&
      currencyCodePair === data.ordersStateAndTransactions.currencyCodePair
    ) {
      await this.setState({ orderState: ordersStateAndTransactions });
      await this.setState({ currencyIndicators: indicators });
    }
  }

  static isInvokeInProgress = false;

  async oneTimeInvoke(newState, method, ...props) {
    if (this.state.isConnected) {
      this.isInvokeInProgress = true;
      await this.wsConnection.send(`${method}|${props}`);
      await this.setState(newState);
      this.isInvokeInProgress = false;
      console.log(method + " completed");
    }
  }

  async retryInvoke(bool, checkField, ...props) {
    await this.oneTimeInvoke(...props);
  }

  async subscribeDashboard() {
    await this.retryInvoke(
      false,
      "subscribedDashboard",
      { subscribedDashboard: true },
      "SubscribeDashboard"
    );
  }

  async unsubscribeDashboard() {
    await this.retryInvoke(
      true,
      "subscribedDashboard",
      { subscribedDashboard: false, dashboardIndicators: null },
      "UnsubscribeDashboard"
    );
  }

  async subscribeBalances() {
    if (!this.state.subscribedBalances) {
      await this.retryInvoke(
        false,
        "subscribedBalances",
        { subscribedBalances: true },
        "SubscribeBalances"
      );
    }
  }

  async unsubscribeBalances() {
    await this.retryInvoke(
      true,
      "subscribedBalances",
      { subscribedBalances: false, balances: null },
      "UnsubscribeBalances"
    );
  }

  async subscribeTradeSignals() {
    await this.retryInvoke(
      false,
      "subscribedTradeSignals",
      { subscribedTradeSignals: true },
      "SubscribeTradeSignals"
    );
  }

  async unsubscribeTradeSignals() {
    await this.retryInvoke(
      true,
      "subscribedTradeSignals",
      { subscribedTradeSignals: false, tradeSignals: null },
      "UnsubscribeTradeSignals"
    );
  }

  async unsubscribePL() {
    if (this.state.subscribedPL) {
      const currentCodes = this.state.subscribedPL.split("|");
      await this.retryInvoke(
        true,
        "subscribedPL",
        { subscribedPL: "", transactions: null, profits: null },
        "UnsubscribePL",
        currentCodes[0],
        currentCodes[1],
        currentCodes[2],
        100
      );
    }
  }

  getSubscribeString(...props) {
    let res = "";

    let isStrings = true;
    props.forEach((prop) => {
      if (typeof prop !== "string") {
        isStrings = false;
      }
    });

    if (!isStrings) return res;

    let isNotEmpty = true;
    props.forEach((prop) => {
      if (prop.length === 0) {
        isNotEmpty = false;
      }
    });

    if (!isNotEmpty) return res;

    props.forEach((prop, index) => {
      res += prop + (index === props.length - 1 ? "" : "|");
    });

    return res;
  }

  async updatePLSubscription(strategyName, exchangeName, currencyCodePair) {
    const newSubscribed = this.getSubscribeString(
      strategyName,
      currencyCodePair,
      exchangeName
    );

    if (this.state.subscribedPL !== newSubscribed) {
      await this.unsubscribePL();

      if (newSubscribed && newSubscribed.length !== 0) {
        await this.retryInvoke(
          false,
          "subscribedPL",
          {
            subscribedPL: newSubscribed,
            exchangeName,
            currencyCodePair,
            strategyName,
          },
          "SubscribePL",
          strategyName,
          currencyCodePair,
          exchangeName,
          100
        );
      }
    }
  }

  async unsubscribeLiquidity() {
    if (this.state.subscribedLiquidity) {
      const currentCodes = this.state.subscribedLiquidity.split("|");
      await this.retryInvoke(
        true,
        "subscribedLiquidity",
        {
          subscribedLiquidity: "",
          orderState: null,
        },
        "UnsubscribeLiquidity",
        currentCodes[0],
        currentCodes[1]
      );
    }
  }

  async updateLiquiditySubscription(exchangeName, currencyCodePair) {
    const newSubscribed = this.getSubscribeString(
      exchangeName,
      currencyCodePair
    );

    if (this.state.subscribedLiquidity !== newSubscribed) {
      await this.unsubscribeLiquidity();

      if (newSubscribed) {
        await this.retryInvoke(
          false,
          "subscribedLiquidity",
          {
            subscribedLiquidity: newSubscribed,
            exchangeName,
            currencyCodePair,
          },
          "SubscribeLiquidity",
          JSON.stringify({
            exchangeId: exchangeName,
            currencyPair: currencyCodePair,
          })
        );
      }
    }
  }

  async unsubscribeVolume() {
    if (this.state.subscribedVolume) {
      const currentCodes = this.state.subscribedVolume.split("|");
      await this.retryInvoke(
        true,
        "subscribedVolume",
        {
          subscribedVolume: "",
          volumeNow: null,
        },
        "UnsubscribeVolume",
        currentCodes[0],
        currentCodes[1]
      );
    }
  }

  async updateVolumeSubscription(exchangeName, currencyCodePair) {
    const newSubscribed = this.getSubscribeString(
      exchangeName,
      currencyCodePair
    );

    if (this.state.subscribedVolume !== newSubscribed) {
      await this.unsubscribeVolume();

      if (newSubscribed) {
        await this.retryInvoke(
          false,
          "subscribedVolume",
          {
            subscribedVolume: newSubscribed,
            exchangeName,
            currencyCodePair,
          },
          "SubscribeVolume",
          currencyCodePair,
          exchangeName
        );
      }
    }
  }

  async updateState(exchangeName, currencyPair, currencyCodePair) {
    await this.setState({
      exchangeName,
      currencyPair,
      currencyCodePair,
    });
  }

  async processAuthorized(message) {
    if (!message.value) {
      this.state.auth = false;
      let refreshToken = localStorage.getItem("refresh_token");
      if (!refreshToken) {
        this.wsConnection.close();
        CryptolpAxios.logout();
        return;
      }

      await CryptolpAxios.loginByRefreshToken({ refreshToken })
        .then(async (response) => {
          const clientType = await CryptolpAxios.getClientType();
          CryptolpAxios.setToken(response.data, clientType.content);
          await this.auth(CryptolpAxios.token);
        })
        .catch((err) => {
          console.error(err);
          this.wsConnection.close();
          CryptolpAxios.logout();
        });
    }
  }
}

export default WsContainer;
