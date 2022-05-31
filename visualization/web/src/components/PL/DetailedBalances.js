import React from "react";
import {Row, Col} from "react-bootstrap";
import "./DetailBalances.css";

function DetailBalances(props) {
    const result = [];
    Object.keys(props.balances).forEach((strategy, index) => {
        const configs = props.balances[strategy];
        result.push(
            <Col className="base-col base-background balanes-col">
                <Col className="base-col strategy-name">{strategy}</Col>
                {getConfigs(configs)}
            </Col>,
        );
    });

    return result;
}

function getConfigs(configs) {
    const result = [];
    Object.keys(configs).forEach((config, index) => {
        const currencies = configs[config];
        result.push(
            <Col md={12} sm xs key={index} className="base-col">
                <Row className="base-row">
                    <Col md={{span: 3, offset: 1}} sm xs className="base-col config-name">
                        {config}
                    </Col>
                    <Col md={{span: 7, offset: 1}} sm xs className="base-col">
                        {getCurrencies(currencies)}
                    </Col>
                </Row>
            </Col>,
        );
    });
    return result;
}

function getCurrencies(currencies) {
    const result = [];
    Object.keys(currencies).forEach((currency, index) => {
        const value = currencies[currency];
        result.push(
            <Row key={index} className="base-row">
                {currency}:{value}
            </Row>,
        );
    });
    return result;
}
