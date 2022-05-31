import React from "react";
import PropTypes from "prop-types";
import utils from "../../utils";
import {Row, Col, Container} from "react-bootstrap";
import {getExchangeImage} from "../Utils/Images";

function Exchange(props) {
    const {
        exchangeName,
        currencyPair,
        spread,
        volumePct,
        totalBid,
        totalAsk,
        ourSpreadMin,
        ourSpreadMax,
        ourTotalBid,
        ourTotalAsk,
    } = props.dashboardIndicators;

    const data = {};
    data.totalAsk = totalAsk;
    data.totalBid = totalBid;
    data.volumePct = utils.round(volumePct, 0);
    data.spread = utils.round(spread, 1);
    data.ourSpreadMin = ourSpreadMin ? utils.round(ourSpreadMin, 1) : "--";
    data.ourSpreadMax = ourSpreadMax ? utils.round(ourSpreadMax, 1) : "--";
    data.ourTotalAsk = ourTotalAsk ? ourTotalAsk : "--";
    data.ourTotalBid = ourTotalBid ? ourTotalBid : "--";

    return (
        <Container className="exchange base-background base-container">
            <Row className="block-exchange-pair">
                <Row className="header-row">
                    <div className="exchange-logo-new">{getExchangeImage(exchangeName, 26)}</div>
                    <div className="exchange-name">{exchangeName}</div>
                    <div className="pair-name">{currencyPair}</div>
                </Row>
            </Row>

            <Row className="base-row title-text title-spread">
                <div className="ml-auto mr-auto">Min Spread</div>
            </Row>

            <Row className="block-exchange">
                <Col className="title-text">%</Col>
                <Col className="title-text">Bid</Col>
                <Col className="title-text">Ask</Col>
            </Row>
            <Row className="block-exchange">
                <Col className="spread-info base-col">{data.spread}</Col>
                <Col className="spread-info base-col">{data.totalAsk}</Col>
                <Col className="spread-info base-col">{data.totalBid}</Col>
            </Row>

            <Row className="base-row title-text title-spread">
                <div className="ml-auto mr-auto">Our Spread</div>
            </Row>

            <Row className="block-exchange">
                <Col className="title-text">%</Col>
                <Col className="title-text">Bid</Col>
                <Col className="title-text">Ask</Col>
            </Row>
            <Row className="block-exchange">
                <Col className="spread-info base-col">
                    {data.ourSpreadMin !== data.ourSpreadMax
                        ? `${data.ourSpreadMin} - ${data.ourSpreadMax}`
                        : `${data.ourSpreadMin}`}
                </Col>
                <Col className="spread-info base-col">{data.ourTotalAsk}</Col>
                <Col className="spread-info base-col">{data.ourTotalBid}</Col>
            </Row>
        </Container>
    );
}

Exchange.propTypes = {
    dashboardIndicators: PropTypes.object,
};

export default Exchange;
