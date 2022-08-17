import React from "react";
import PropTypes from "prop-types";
import utils from "../../utils";
import { Row, Col } from "react-bootstrap";

const Exchange = (props) => {
  const percent = Number(props.total)
    ? utils.round((props.exchange.value * 100) / props.total, 3)
    : 0;
  return (
    <Row md={12} className="market_row base-row">
      <div
        className={`indicator-coin ${
          percent && percent < 10 ? "critical" : ""
        }`}
        style={{ width: percent + "%" }}
      ></div>
      <Col md={3} className="left-part base-col">
        <h5 className="market-title">
          <strong className="market-title-semi-bold">
            {props.exchange.exchangeId}
          </strong>
        </h5>
        <div className="percent">{percent}%</div>
      </Col>
      <Col md={9}>
        <h4 className="coin-amount">
          <strong className="bold-text">{Number(props.exchange.value)}</strong>
        </h4>
      </Col>
    </Row>
  );
};

Exchange.propTypes = {
  total: PropTypes.number.isRequired,
  exchange: PropTypes.object.isRequired,
};

export default Exchange;
