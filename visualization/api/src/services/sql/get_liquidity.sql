SELECT id, json
FROM liquidity_order_book
WHERE ((json ->> 'exchange_id')::TEXT = $1) AND ((json ->> 'currency_pair')::TEXT = $2)
ORDER BY insert_time DESC, id DESC
LIMIT 1;
