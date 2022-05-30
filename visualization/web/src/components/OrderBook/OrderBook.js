import React from "react";
import PropTypes from "prop-types";
import Spinner from "../../controls/Spinner";
import Orders from "./Orders";
import {Row} from "react-bootstrap";
import "./OrderBook.css";

function OrderBook(props) {
    if (!props.symbol || !props.orderState) {
        return <Spinner />;
    }

    const leftSeparator = props.orderState.buy.snapshot.length > props.orderState.sell.snapshot.length;
    return (
        <Row className="base-background base-row">
            <Orders
                separator={!leftSeparator ? "separator-right" : ""}
                highlightClass="indicator_instant_sell"
                titleClass="red_title"
                symbol={props.symbol}
                orders={props.orderState.sell}
                desiredAmount={props.orderState.desiredAmount}
                leftSide
            />
            <Orders
                separator={leftSeparator ? "separator-left" : ""}
                titleClass="green_title"
                highlightClass="indicator_instant_buy"
                symbol={props.symbol}
                orders={props.orderState.buy}
                desiredAmount={props.orderState.desiredAmount}
            />
        </Row>
    );
}

OrderBook.propTypes = {
    orderState: PropTypes.object,
};

export default OrderBook;
