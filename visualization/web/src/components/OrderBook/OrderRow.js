import React from "react";
import PropTypes from "prop-types";
import utils from "../../utils";
import {Row, Col} from "react-bootstrap";

function OrderRow(props) {
    const price = props.snapshot[0];
    const amount = props.snapshot[1];

    let percentage = props.desiredAmount ? (props.currentAmount * 100) / props.desiredAmount : 0;

    if (percentage > 100) percentage = 100;

    const priceText = utils.round(price, 8);
    const amountText = utils.round(amount, 8);

    return (
        <Row className="transaction_row">
            <div className={props.highlightClass} style={{width: percentage + "%"}}></div>
            <Col md={6} sm xs className="order-price-amount justify-content-md-center">
                <span>{priceText}</span>
            </Col>
            <Col md={6} sm xs className="order-price-amount justify-content-md-center">
                <span>{amountText}</span>
            </Col>
        </Row>
    );
}

OrderRow.propTypes = {
    highlightClass: PropTypes.string.isRequired,
    snapshot: PropTypes.array.isRequired,
    desiredAmount: PropTypes.number.isRequired,
};

export default OrderRow;
