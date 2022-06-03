import React from "react";
import { withRouter } from "react-router-dom";
import Spinner from "../../controls/Spinner";
import Body from "./body";

class Explanation extends React.Component {
  async componentDidMount() {
    await this.update();
  }

  async componentDidUpdate() {
    await this.update();
  }

  async update() {
    const { exchange, explanation } = this.props;
    const exState = exchange.state;
    const expState = explanation.state;

    if (
      exState.exchangeName &&
      exState.currencyCodePair &&
      (exState.exchangeName !== expState.exchange ||
        exState.currencyCodePair !== expState.currencyCodePair)
    ) {
      await explanation.updateExplanations(
        exState.exchangeName,
        exState.currencyCodePair
      );
    }
  }

  render() {
    const {
      state: { loading, explanations },
    } = this.props.explanation;

    return !loading && explanations ? (
      <Body explanations={explanations} />
    ) : (
      <Spinner />
    );
  }
}

export default withRouter(Explanation);
