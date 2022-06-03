import React from "react";
import { withRouter } from "react-router-dom";
import { Col } from "react-bootstrap";
import Spinner from "../../controls/Spinner";
import GraphNow from "./GraphNow";

class PLGraph extends React.Component {
  async componentDidMount() {
    await this.update();
  }

  async componentDidUpdate() {
    await this.update();
  }

  async update() {
    const { exchange, data } = this.props;
    const exState = exchange.state;
    const dataState = data.state;

    if (
      exState.exchangeName !== dataState.exchangeName ||
      exState.currencyCodePair !== dataState.currencyCodePair
    ) {
      await this.props.data.loadData(
        exState.exchangeName,
        exState.currencyCodePair
      );
    }
  }

  render() {
    const {
      state: { loading, data },
    } = this.props.data;
    return !loading && data ? (
      <Col className="base-container">
        <GraphNow data={data} />
      </Col>
    ) : (
      <Spinner />
    );
  }
}

export default withRouter(PLGraph);
