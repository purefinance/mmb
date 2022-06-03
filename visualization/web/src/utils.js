import { DateTime } from "luxon";

// https://stackoverflow.com/a/21323513
// this implementation was chosen due to a special case: value = -4.6806262017749e-7
function round(value, exp) {
  if (typeof exp === "undefined" || +exp === 0) return Math.round(value);

  value = +value;
  exp = +exp;

  if (isNaN(value) || !(typeof exp === "number" && exp % 1 === 0)) return NaN;

  // Shift
  value = value.toString().split("e");
  value = Math.round(+(value[0] + "e" + (value[1] ? +value[1] + exp : exp)));

  // Shift back
  value = value.toString().split("e");
  return +(value[0] + "e" + (value[1] ? +value[1] - exp : -exp));
}

function formatToUsd(value) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
  }).format(value);
}

function normalCodePair(code) {
  if (code) {
    return code.replace("-", "/");
  } else {
    return code;
  }
}
function urlCodePair(code) {
  if (code) {
    return code.replace("/", "-");
  } else {
    return code;
  }
}

async function checkParameters(props) {
  const { interval, exchangeName, urlCurrencyCodePair } = props.match.params;
  const exchangeState = props.exchange.state;

  if (
    !exchangeState.exchanges ||
    !exchangeState.exchangeName ||
    !exchangeState.currencyPair
  )
    return;

  const currencyCodePair = this.normalCodePair(urlCurrencyCodePair);

  let newPath = props.path;
  if (props.isNeedInterval) {
    const validIntervals = ["now", "day", "week", "month"];
    newPath +=
      interval && validIntervals.indexOf(interval) >= 0
        ? `/${interval}`
        : "/day";
  }

  if (
    props.isNeedExchange &&
    props.exchange.state.exchanges &&
    exchangeState.exchangeName
  ) {
    if (exchangeName && exchangeState.exchangeName !== exchangeName) {
      let isValidExchange = false;
      props.exchange.state.exchanges.forEach((ex) => {
        if (ex.name === exchangeName) {
          isValidExchange = true;
        }
      });

      if (props.path === "/profits") {
        newPath +=
          isValidExchange || exchangeName === "all"
            ? `/${exchangeName}`
            : `/${exchangeState.exchangeName}`;
      } else {
        const newExchange =
          exchangeState.exchangeName === "all"
            ? props.exchange.state.exchanges[0].name
            : exchangeState.exchangeName;
        newPath += isValidExchange ? `/${exchangeName}` : `/${newExchange}`;
      }
    } else {
      const newExchange =
        exchangeState.exchangeName === "all" && props.path !== "/profits"
          ? props.exchange.state.exchanges[0].name
          : exchangeState.exchangeName;
      newPath += `/${newExchange}`;
    }
  }

  if (
    props.isNeedCurrencyCodePair &&
    props.exchange.state.symbols &&
    exchangeState.currencyCodePair
  )
    if (
      currencyCodePair &&
      exchangeState.currencyCodePair !== currencyCodePair
    ) {
      let isValidPair = false;
      props.exchange.state.symbols.forEach((s) => {
        if (s.currencyCodePair === currencyCodePair) {
          isValidPair = true;
        }
      });

      if (props.path === "/profits") {
        newPath +=
          isValidPair || currencyCodePair === "all"
            ? `/${this.urlCodePair(currencyCodePair)}`
            : `/${this.urlCodePair(exchangeState.currencyCodePair)}`;
      } else {
        const newCurrencyPair =
          exchangeState.currencyCodePair === "all"
            ? props.exchange.state.symbols[0].currencyCodePair
            : exchangeState.currencyCodePair;
        newPath += isValidPair
          ? `/${this.urlCodePair(currencyCodePair)}`
          : `/${this.urlCodePair(newCurrencyPair)}`;
      }
    } else {
      const newCurrencyPair =
        exchangeState.currencyCodePair === "all" && props.path !== "/profits"
          ? props.exchange.state.symbols[0].currencyCodePair
          : exchangeState.currencyCodePair;
      newPath += `/${this.urlCodePair(newCurrencyPair)}`;
    }

  if (newPath && props.history.location.pathname !== newPath) {
    pushNewLinkToHistory(props.history, newPath);
  } else {
    if (interval && interval !== props.exchange.state.interval) {
      await props.exchange.setInterval(interval);
    }

    if (
      exchangeName === "all" &&
      props.path === "/profits" &&
      props.exchange.state.exchangeName !== "all"
    ) {
      await props.exchange.updateSelectedExchange(-1);
    } else {
      if (props.exchange.state.exchanges)
        props.exchange.state.exchanges.forEach(async (ex, index) => {
          if (
            ex.name === exchangeName &&
            props.exchange.state.exchangeName !== exchangeName
          ) {
            await props.exchange.updateSelectedExchange(index);
          }
        });
    }

    if (
      currencyCodePair === "all" &&
      props.path === "/profits" &&
      props.exchange.state.currencyCodePair !== "all"
    ) {
      await props.exchange.updateSelectedSymbol(-1);
    } else {
      if (props.exchange.state.symbols)
        props.exchange.state.symbols.forEach(async (s, index) => {
          if (
            s.currencyCodePair === currencyCodePair &&
            props.exchange.state.currencyCodePair !== currencyCodePair
          ) {
            await props.exchange.updateSelectedSymbol(index);
          }
        });
    }
  }
}

function parseDateTime(dateTime) {
  return DateTime.fromISO(dateTime, { zone: "utc" }).toLocal();
}

function toLocalTime(dateTime) {
  return parseDateTime(dateTime).toLocaleString(DateTime.TIME_WITH_SECONDS);
}

function toLocalDateTime(dateTime) {
  return parseDateTime(dateTime).toLocaleString(
    DateTime.DATETIME_SHORT_WITH_SECONDS
  );
}

function pushNewLinkToHistory(history, newLink) {
  if (history.location.pathname === newLink) {
    history.replace(newLink);
  } else {
    history.push(newLink);
  }
}

export default {
  pushNewLinkToHistory,
  toLocalDateTime,
  checkParameters,
  normalCodePair,
  parseDateTime,
  toLocalTime,
  formatToUsd,
  urlCodePair,
  round,
};
