import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class PLGraphContainer extends Container {
  constructor(props) {
    super(props);
    this.state = {
      loading: false,
      data: null,
      exchangeName: null,
      currencyCodePair: null,
    };

    this.loadData = this.loadData.bind(this);
  }

  async loadData(exchangeName, currencyCodePair) {
    if (this.state.loading) {
      return;
    }

    await this.setState({ loading: true });
    const data = await CryptolpAxios.getPLGraph(exchangeName, currencyCodePair);
    await this.setState({
      loading: false,
      data,
      exchangeName,
      currencyCodePair,
    });
  }
}

export default PLGraphContainer;
