import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class LiquidityContainer extends Container {
  constructor(props) {
    super(props);
    this.state = {
      currencyCodePair: "",
      currencyPair: "",
      exchangeName: "",
      interval: "",
      preprocessedOrderBook: null,
      liquidityIndicators: null,
    };
  }

  async updateSelection(
    exchangeName,
    currencyPair,
    currencyCodePair,
    interval
  ) {
    await this.setState({
      interval: interval,
      exchangeName: exchangeName,
      currencyPair: currencyPair,
      currencyCodePair: currencyCodePair,
      liquidityIndicators: null,
      preprocessedOrderBook: null,
    });

    if (interval !== "now" && exchangeName && currencyPair) {
      const result = await CryptolpAxios.getLiquidityIndicators(
        exchangeName,
        currencyPair,
        interval
      );
      await this.setState({
        preprocessedOrderBook: result.preprocessedOrderBook,
        liquidityIndicators: result.liquidityIndicators,
      });
    }
  }

  async clearData() {
    await this.setState({
      interval: "",
      exchangeName: "",
      currencyPair: "",
      currencyCodePair: "",
      liquidityIndicators: null,
      preprocessedOrderBook: null,
    });
  }
}

export default LiquidityContainer;
