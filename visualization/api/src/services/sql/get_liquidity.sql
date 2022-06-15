SELECT id, json
FROM liquidity_order_books
WHERE ((json ->> 'exchange_id')::text = $1)
  AND ((json ->> 'currency_pair')::text = $2)
ORDER BY insert_time DESC, id DESC
LIMIT 1;
