import React from "react";
import { withRouter } from "react-router-dom";
import Spinner from "../../controls/Spinner";
import Body from "./body";
import { Indicators } from "../../components/Volume";
import VolumeNow from "../VolumeNow";
import { Container } from "react-bootstrap";

class Volume extends React.Component {
  async componentDidMount() {
    await this.update();
  }

  async componentDidUpdate() {
    await this.update();
  }

  async update() {
    const { exchange, volume } = this.props;
    const exState = exchange.state;
    const volState = volume.state;

    if (
      exState.exchangeName !== volState.exchangeName ||
      exState.currencyPair !== volState.currencyPair ||
      exState.interval !== volState.interval
    ) {
      //Temporarily unused
      // await volume.updateVolume(exState.currencyPair, exState.exchangeName, exState.interval);
    }
  }

  async componentWillUnmount() {
    await this.props.volume.clearData();
  }

  render() {
    const {
      state: { symbol, interval },
    } = this.props.exchange;
    const {
      state: { volumeIndicators, currencyPair, exchangeName },
    } = this.props.volume;

    if (interval === "now") {
      return (
        <VolumeNow
          exchange={this.props.exchange}
          volume={this.props.volume}
          ws={this.props.ws}
        />
      );
    }

    return (
      <Container className="base-container">
        <Indicators
          volume={
            volumeIndicators &&
            volumeIndicators.data &&
            volumeIndicators.data.volume
          }
          symbol={symbol}
        />
        {currencyPair && exchangeName ? (
          <Body volume={this.props.volume} exchange={this.props.exchange} />
        ) : (
          <Spinner />
        )}
      </Container>
    );
  }
}

export default withRouter(Volume);
