import React from "react";
import PropTypes from "prop-types";
import Indicator from "../Indicator/Indicator";
import utils from "../../utils";
import { IndicatorErrorBoundary } from "../../errorBoundaries";
import { Row } from "react-bootstrap";

function Indicators(props) {
  let loading = props.orderState === null;
  let data = { loading };
  if (!loading) {
    const { indicators } = props.orderState;
    if (indicators) {
      data.volumePct = utils.round(indicators.volumePct, 0);
      data.bidPct = utils.round(indicators.bidPct, 0);
      data.askPct = utils.round(indicators.askPct, 0);

      data.spread = indicators.spread
        ? utils.round(indicators.spread, 1)
        : "--";
      data.totalVolume = indicators.totalVolume
        ? utils.round(indicators.totalVolume, 3)
        : "-–";
      data.totalBid = indicators.totalBid
        ? utils.round(indicators.totalBid, 3)
        : "--";
      data.totalAsk = indicators.totalAsk
        ? utils.round(indicators.totalAsk, 3)
        : "--";
    }
    data.symbol = props.symbol ? props.symbol.amountCurrencyCode : "";
  }

  const curIndicators = props.currencyIndicators;
  const hasBalanceIndicators =
    !loading && curIndicators && curIndicators.balanceIndicators;
  return (
    <Row className="base-container">
      <IndicatorErrorBoundary>
        <Indicator first data={data.spread} postfix="%" title="Spread" />

        <Indicator
          data={data.totalVolume}
          percentage={data.volumePct || 0}
          postfix={data.symbol}
          title="Total Volume"
        />

        <Indicator
          data={data.totalAsk}
          percentage={data.askPct || 0}
          postfix={data.symbol}
          title="Asks Volume"
        />

        <Indicator
          last
          data={data.totalBid}
          percentage={data.bidPct || 0}
          postfix={data.symbol}
          title="Bids Volume"
        />

        {hasBalanceIndicators &&
          curIndicators.balanceIndicators.map((indicator, index) => {
            const { currencyCode, reservationAmount, limitAmount } = indicator;
            const fmtBalance = (balance) => utils.round(balance, 3);

            let tooltip = "";
            let indicatorsData = "";
            let filledAmount = 0;
            const amountCurrencyCode = curIndicators.amountCurrencyCode;
            const positionData = curIndicators.position;
            if (positionData && positionData.position !== undefined) {
              if (amountCurrencyCode === currencyCode) {
                filledAmount = positionData.position;
                indicatorsData += `F: ${fmtBalance(filledAmount)} / `;
                tooltip += "Filled / ";
              } else {
                filledAmount = -positionData.position;
              }
            }

            indicatorsData += `R: ${fmtBalance(reservationAmount)}`;
            tooltip += "Reserved";

            let percent;
            if (limitAmount !== undefined) {
              indicatorsData += ` / L: ${fmtBalance(limitAmount)}`;
              percent =
                Math.max(
                  0,
                  Math.min(1, (filledAmount + reservationAmount) / limitAmount)
                ) * 100;
              tooltip += " / Limit";
            }

            const title = `${currencyCode} amount`;
            return (
              <Indicator
                key={index}
                data={indicatorsData}
                percentage={utils.round(percent, 0)}
                title={title}
                tooltip={tooltip}
              />
            );
          })}

        {hasBalanceIndicators && getPositionCurrencyIndicator(curIndicators)}
        {hasBalanceIndicators &&
          getLiquidationIndicator(
            curIndicators,
            props.orderState,
            props.symbol
          )}
      </IndicatorErrorBoundary>
    </Row>
  );
}

function getIndicator(curIndicators, index) {
  const amountCurrencyCode = curIndicators.amountCurrencyCode;
  const position = curIndicators.position ? curIndicators.position.position : 0;

  const indicator = curIndicators.balanceIndicators[index];
  const filledAmount =
    indicator.currencyCode === amountCurrencyCode ? position : 0;
  indicator.amount = filledAmount + indicator.reservationAmount;
  if (indicator.limitAmount) {
    indicator.normAmount = indicator.amount / indicator.limitAmount;
  }
  return indicator;
}

function getPositionCurrencyIndicator(curIndicators) {
  const balanceIndicators = curIndicators.balanceIndicators;
  if (!balanceIndicators || !balanceIndicators[0] || !balanceIndicators[1]) {
    return null;
  }

  const ind1 = getIndicator(curIndicators, 0);
  const ind2 = getIndicator(curIndicators, 1);

  let indicatorsData;
  if (!ind1.limitAmount || !ind2.limitAmount) {
    indicatorsData = `Absolute: ${utils.round(ind2.amount - ind1.amount, 3)}`;
  } else {
    const position =
      Math.max(-1, Math.min(1, ind2.normAmount - ind1.normAmount)) * 100;
    indicatorsData = `Relative: ${utils.round(position, 0)}%`;
  }
  return (
    <Indicator
      key={3}
      data={indicatorsData}
      title={`${ind1.currencyCode}/${ind2.currencyCode} position`}
    />
  );
}

function getLiquidationIndicator(curIndicators, orderState, symbol) {
  if (!curIndicators.position || !orderState || !orderState.indicators) {
    return;
  }

  const actualPrice = utils.round(
    orderState.indicators.mediumPrice,
    symbol.pricePrecision
  );
  const liquidationPrice = curIndicators.position.liquidationPrice;
  const avgEntryPrice = curIndicators.position.avgEntryPrice;

  let percent;
  if (avgEntryPrice < liquidationPrice) {
    percent =
      Math.max(
        0,
        (actualPrice - avgEntryPrice) / (liquidationPrice - avgEntryPrice)
      ) * 100;
  } else {
    percent =
      Math.max(
        0,
        (avgEntryPrice - actualPrice) / (avgEntryPrice - liquidationPrice)
      ) * 100;
  }

  const content = `${avgEntryPrice} / ${actualPrice} / ${liquidationPrice}`;
  const title = `Liquidation (${
    curIndicators.position.side === 1
      ? "Buy"
      : curIndicators.position.side === 2
      ? "Sell"
      : "None"
  })`;
  return (
    <Indicator
      key={4}
      data={content}
      percentage={utils.round(percent, 0)}
      title={title}
      tooltip="Entry / Actual / Liquidation"
    />
  );
}

Indicators.propTypes = {
  orderState: PropTypes.object,
};

export default Indicators;
