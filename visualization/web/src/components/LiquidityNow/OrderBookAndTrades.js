import React from "react";
import PropTypes from "prop-types";
import { Col, Row } from "react-bootstrap";
import Spinner from "../../controls/Spinner";
import OrderBook from "../OrderBook/OrderBook";
import Trades from "../Trades/Trades";

function OrderBookAndTrades(props) {
  return props.orderState && props.symbol ? (
    <Row className="base-container base-row">
      <Col md={5} sm={12} xs={12}>
        <Row className="justify-content-md-center">
          <strong className="topic">Order Book</strong>
        </Row>
        <OrderBook orderState={props.orderState} symbol={props.symbol} />
      </Col>
      <Col md={7} sm={12} xs={12}>
        <Row className="justify-content-md-center">
          <strong className="topic">Trades</strong>
        </Row>
        <Trades
          transactions={props.orderState.transactions}
          dashboard={false}
        />
      </Col>
    </Row>
  ) : (
    <Spinner />
  );
}

OrderBook.propTypes = {
  orderState: PropTypes.object,
};

export default OrderBookAndTrades;
