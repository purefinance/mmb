import React from "react";
import Indicator from "../Indicator/Indicator";
import utils from "../../utils";

function PLIndicator(props) {
  const rawProfit = `${utils.formatToUsd(props.raw || 0)}`;
  const profitOverMarket = `${utils.formatToUsd(props.overMarket || 0)}`;
  const result = `${rawProfit} / ${profitOverMarket}`;
  return (
    <Indicator data={result} title={`${props.title} (raw / over market)`} />
  );
}

export default PLIndicator;
