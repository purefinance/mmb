CREATE TABLE transactions (
    id bigserial PRIMARY KEY,
    insert_time timestamp WITH TIME ZONE NOT NULL DEFAULT now(),
    version int,
    json jsonb NOT NULL
);

CREATE INDEX transactions__insert_time_idx ON transactions USING btree (insert_time);
CREATE INDEX transactions__market_id_exchange_id_idx ON transactions USING btree (((json #>> '{market_id, exchange_id}')::text));
CREATE INDEX transactions__market_id_currency_pair_idx ON transactions USING btree (((json #>> '{market_id, currency_pair}')::text));
CREATE INDEX transactions__transaction_creation_time_idx ON transactions USING btree (((json ->> 'transaction_creation_time')::text));

GRANT ALL PRIVILEGES ON TABLE transactions TO dev;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO dev;