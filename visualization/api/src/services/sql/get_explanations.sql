SELECT id, insert_time, json FROM disposition_explanations
WHERE ((json ->> 'exchange_id')::text = $1)
  AND ((json ->> 'currency_pair')::text = $2)
ORDER BY insert_time
limit $3