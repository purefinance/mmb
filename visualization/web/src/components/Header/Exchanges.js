import React from "react";
import { withRouter } from "react-router-dom";
import Dropdown from "../../controls/Dropdown/Dropdown";
import { Row, Col } from "react-bootstrap";
import { getExchangeImage } from "../Utils/Images";
import utils from "../../utils";

class Exchanges extends React.Component {
  render() {
    const { interval, exchangeName, currencyCodePair, exchanges } =
      this.props.exchange.state;
    const { isNeedInterval, isNeedExchange, isNeedCurrencyCodePair } =
      this.props;

    const exchangeElements = [];
    if (exchanges) {
      if (this.props.needAll)
        exchangeElements.push(
          <div
            key={0}
            className="dropdown-text-inner currency"
            onClick={() =>
              utils.pushNewLinkToHistory(
                this.props.history,
                `${this.props.path}/all/${utils.urlCodePair(currencyCodePair)}`
              )
            }
          >
            All
          </div>
        );
      exchanges.forEach((exchange, index) => {
        let newPath = `${this.props.path}`;
        newPath += isNeedInterval ? `/${interval}` : "";
        newPath += isNeedExchange ? `/${exchange.name}` : "";
        newPath += isNeedCurrencyCodePair
          ? `/${utils.urlCodePair(currencyCodePair)}`
          : "";

        exchangeElements.push(
          <Row
            key={index + 1}
            className="row-center"
            onClick={() =>
              utils.pushNewLinkToHistory(this.props.history, newPath)
            }
          >
            <Col md={5} sm xs>
              {getExchangeImage(exchange.name, 32)}
            </Col>
            <div className="dropdown-text-inner">{exchange.name}</div>
          </Row>
        );
      });
    }

    return (
      <Dropdown
        id="ExchangeDropdown"
        headerText="dropdown-text"
        value={exchangeName === "all" ? "All exchanges" : exchangeName}
        image={
          exchangeName && exchangeName !== "all" ? (
            getExchangeImage(exchangeName, 38)
          ) : (
            <div style={{ width: 38 }} />
          )
        }
      >
        {exchangeElements}
      </Dropdown>
    );
  }
}

export default withRouter(Exchanges);
