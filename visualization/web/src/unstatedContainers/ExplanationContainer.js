import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class ExplanationContainer extends Container {
  constructor(props) {
    super(props);
    this.state = {
      loading: false,
      explanations: null,
      exchange: "",
      currencyCodePair: "",
    };

    this.updateExplanations = this.updateExplanations.bind(this);
  }
  async updateExplanations(exchange, currencyCodePair) {
    await this.setState({
      loading: true,
      explanations: null,
      exchange: exchange,
      currencyCodePair: currencyCodePair,
    });

    this.lastPromise = CryptolpAxios.getExplanations(
      exchange,
      currencyCodePair
    );

    this.lastPromise.then(async (result) => {
      if (
        result.exchangeName === this.state.exchange &&
        result.currencyCodePair === this.state.currencyCodePair
      ) {
        await this.setState({
          loading: false,
          explanations: result.explanations,
        });
      }
    });
  }
}

export default ExplanationContainer;
