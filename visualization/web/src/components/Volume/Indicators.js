import React from "react";
import PropTypes from "prop-types";
import Indicator from "../Indicator/Indicator";
import {Row} from "react-bootstrap";

const Indicators = (props) => {
    return (
        <Row className="base-container">
            <Indicator
                first
                data={props.volume}
                additionalclasses="volume"
                postfix={props.symbol && props.symbol.amountCurrencyCode}
                title="Volume"
            />
        </Row>
    );
};

Indicators.propTypes = {
    volume: PropTypes.number,
    symbol: PropTypes.object,
};

export default Indicators;
