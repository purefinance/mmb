import React from "react";
import { getExchangeImagePath, getCoinImagePath } from "../../strings";
import "./Images.css";

export function getExchangeImage(name, size) {
  return (
    <img
      src={getExchangeImagePath(name)}
      alt={name}
      width={size}
      height={size}
      className="exchange-image"
      onError={(ev) => {
        ev.target.src = "/images/CoinMarketCapIcons/exchanges/default.png";
      }}
    />
  );
}

export function getCoinImage(name, size) {
  return (
    <img
      src={getCoinImagePath(name)}
      alt={name}
      width={size}
      height={size}
      className="coin-logo"
      onError={(ev) => {
        ev.target.src = "/images/CoinMarketCapIcons/exchanges/default.png";
      }}
    />
  );
}
