import React from "react";
import { Container } from "react-bootstrap";
import { BodyErrorBoundary } from "../../errorBoundaries";
import Body from "./body";

class PL extends React.Component {
  async componentDidMount() {
    await this.updateSubscription();
  }

  async componentDidUpdate() {
    await this.updateSubscription();
  }

  async componentWillUnmount() {
    await this.props.ws.unsubscribePL();
  }

  async updateSubscription() {
    const { exchange, ws } = this.props;
    const exchangeState = exchange.state;
    if (
      exchangeState.strategyName &&
      exchangeState.exchangeName &&
      exchangeState.currencyCodePair
    ) {
      await ws.updatePLSubscription(
        exchangeState.strategyName,
        exchangeState.exchangeName,
        exchangeState.currencyCodePair
      );
    }
  }

  render() {
    return (
      <Container className="base-container">
        <BodyErrorBoundary>
          <Body exchange={this.props.exchange} ws={this.props.ws} />
        </BodyErrorBoundary>
      </Container>
    );
  }
}

export default PL;
