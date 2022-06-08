CREATE TABLE liquidity_order_book (
    id bigserial PRIMARY KEY,
    insert_time timestamp WITH TIME ZONE NOT NULL,
    version int,
    json jsonb NOT NULL
);

CREATE INDEX insert_time_idx ON liquidity_order_book USING btree (insert_time);
CREATE INDEX exchange_id_idx ON liquidity_order_book USING btree (((json ->> 'exchange_id')::TEXT));
CREATE INDEX currency_pair_idx ON liquidity_order_book USING btree (((json ->> 'currency_pair')::TEXT));

GRANT ALL PRIVILEGES ON TABLE liquidity_order_book TO dev;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO dev;