SELECT id, json
FROM transactions
WHERE ((json -> 'market_id' ->> 'exchange_id')::text = $1)
  AND ((json -> 'market_id' ->> 'currency_pair')::text = $2)
ORDER BY insert_time DESC, id DESC
LIMIT $3
