import React from "react";
import BotParametrs from "../../components/Signals/BotParametrs";
import Spinner from "../../controls/Spinner";

class Signals extends React.Component {
  async componentDidMount() {
    await this.update();
  }

  async componentDidUpdate() {
    await this.update();
  }

  async componentWillUnmount() {
    await this.props.signals.clearState();
  }

  async update() {
    const exchangeState = this.props.exchange.state;
    const signalsState = this.props.signals.state;

    if (
      exchangeState.exchangeName &&
      exchangeState.currencyPair &&
      !signalsState.loading &&
      (signalsState.exchangeName !== exchangeState.exchangeName ||
        signalsState.currencyPair !== exchangeState.currencyPair)
    ) {
      await this.props.signals.updateSignals(
        exchangeState.exchangeName,
        exchangeState.currencyPair
      );
    }
  }

  render() {
    const {
      state: { loading, signals },
    } = this.props.signals;

    return (
      <div className="section-3">
        <div className="container-chart-and-transactions base-container">
          {!loading ? <BotParametrs signals={signals} /> : <Spinner />}
        </div>
      </div>
    );
  }
}

export default Signals;
