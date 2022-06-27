CREATE TABLE liquidity_order_books (
    id bigserial PRIMARY KEY,
    insert_time timestamp WITH TIME ZONE NOT NULL DEFAULT now(),
    version int,
    json jsonb NOT NULL
);

CREATE INDEX liquidity_order_books__insert_time_idx ON liquidity_order_books USING btree (insert_time);
CREATE INDEX liquidity_order_books__exchange_id_idx ON liquidity_order_books USING btree (((json ->> 'exchange_id')::text));
CREATE INDEX liquidity_order_books__currency_pair_idx ON liquidity_order_books USING btree (((json ->> 'currency_pair')::text));

GRANT ALL PRIVILEGES ON TABLE liquidity_order_books TO dev;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO dev;