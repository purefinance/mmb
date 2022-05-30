import React from "react";
import Indicator from "../Indicator/Indicator";
import {Row} from "react-bootstrap";

const Indicators = (props) => {
    let loading = props.liquidityIndicators === null;
    let data = {loading};
    if (!loading) {
        data.minSpread = props.liquidityIndicators.minSpread;
        data.maxSpread = props.liquidityIndicators.maxSpread;
        data.averageSpread = props.liquidityIndicators.averageSpread;
        data.volume = props.liquidityIndicators.volume;
        data.amountCurrencyCode = props.symbol.amountCurrencyCode;
    }
    return (
        <Row className="base-container">
            <Indicator first data={data.minSpread} postfix="%" title="Min. Spread" />
            <Indicator data={data.maxSpread} postfix="%" title="Max. Spread" />
            <Indicator data={data.averageSpread} postfix="%" title="Average Spread" />
            <Indicator last data={data.volume} postfix={data.amountCurrencyCode} title="Volume" />
        </Row>
    );
};

export default Indicators;
