import React from "react";
import { withRouter } from "react-router-dom";
import Body from "./body";
import Spinner from "../../controls/Spinner";
import { Container } from "react-bootstrap";

class VolumeNow extends React.Component {
  async componentDidMount() {
    await this.updateSubscription();
  }
  async componentDidUpdate() {
    await this.updateSubscription();
  }
  async componentWillUnmount() {
    await this.props.ws.unsubscribeVolume();
  }
  async updateSubscription() {
    const { exchange, ws } = this.props;
    const exchangeState = exchange.state;
    if (exchangeState.exchangeName && exchangeState.currencyCodePair) {
      await ws.updateVolumeSubscription(
        exchangeState.exchangeName,
        exchangeState.currencyCodePair
      );
    }
  }

  render() {
    return (
      <Container className="base-container">
        {this.props.ws.state.isConnected ? (
          <Body
            volume={this.props.volume}
            exchange={this.props.exchange}
            ws={this.props.ws}
          />
        ) : (
          <Spinner />
        )}
      </Container>
    );
  }
}

export default withRouter(VolumeNow);
