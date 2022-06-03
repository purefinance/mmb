import React from "react";
import PropTypes from "prop-types";
import OrderRow from "./OrderRow";
import { Col, Row } from "react-bootstrap";

function Orders(props) {
  const rows = [];
  props.orders.snapshot.forEach((snapshot, idx) => {
    let currentAmount = 0;
    props.orders.orders
      .filter((e) => e.price === snapshot[0])
      .forEach((e) => {
        currentAmount += e.amount;
      });
    if (rows.length <= 20 || currentAmount > 0) {
      rows.push(
        <OrderRow
          key={idx}
          highlightClass={props.highlightClass}
          snapshot={snapshot}
          currentAmount={currentAmount}
          desiredAmount={props.desiredAmount}
          leftSide={props.leftSide}
        />
      );
    }
  });

  return (
    <Col className={`base-orders ${props.separator ? props.separator : ""}`}>
      <Row className="title_block_instant">
        <Col className="title_block_general amount">
          <div className="title_regulare_instant">Price</div>
        </Col>
        <Col className="title_block_general amount">
          <div className="title_regulare_instant">Amount</div>
        </Col>
      </Row>
      <Row className="div_transactions">{rows}</Row>
    </Col>
  );
}

Orders.propTypes = {
  separator: PropTypes.string.isRequired,
  titleClass: PropTypes.string.isRequired,
  highlightClass: PropTypes.string.isRequired,
  symbol: PropTypes.object.isRequired,
  orders: PropTypes.object.isRequired,
  desiredAmount: PropTypes.number.isRequired,
  id: PropTypes.string,
};

export default Orders;
