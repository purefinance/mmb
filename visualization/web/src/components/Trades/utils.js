import { removeDuplicates, orderByDate } from "../../controls/functions";

function getBefore(transactions) {
  return transactions[transactions.length - 1].dateTime;
}

function concatTrades(oldTrades, newTrades) {
  let trades = oldTrades;
  trades = trades.concat(newTrades);
  trades = removeDuplicates(trades, "id");
  trades = orderByDate(trades);
  return trades;
}

export default { getBefore, concatTrades };
