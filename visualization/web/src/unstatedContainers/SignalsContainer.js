import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class SignalsContainer extends Container {
  state = {
    loading: true,
    signals: null,
    exchangeName: "",
    currencyPair: "",
  };

  async updateSignals(exchangeName, currencyPair) {
    await this.setState({
      loading: true,
      signals: null,
      exchangeName,
      currencyPair,
    });
    const result = await CryptolpAxios.getSignals(exchangeName, currencyPair);
    await this.setState({ loading: false, signals: result });
  }

  async clearState() {
    await this.setState({
      loading: true,
      signals: null,
      exchangeName: "",
      currencyPair: "",
    });
  }
}

export default SignalsContainer;
