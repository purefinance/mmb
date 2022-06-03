import React from "react";
import PropTypes from "prop-types";
import Exchange from "./Exchange";
import { Col, Row } from "react-bootstrap";
import { getCoinImage } from "../Utils/Images";

const Currency = (props) => {
  return (
    <Col md={6} sm={6} xs={12} className="currency-exchange-container">
      <Row className="currency-row">
        {getCoinImage(props.currencyCode, 32)}
        <h4 className="coin-name">{props.currencyCode}</h4>
      </Row>
      <Col className="base-col base-background hidden border-shadow">
        {props.data.exchanges.map((data) => (
          <Exchange
            total={props.data.total}
            exchange={data}
            key={data.exchangeId}
          />
        ))}
      </Col>
    </Col>
  );
};

Currency.propTypes = {
  currencyCode: PropTypes.string.isRequired,
  data: PropTypes.object.isRequired,
};

export default Currency;
