CREATE TABLE IF NOT EXISTS TABLE_NAME (
                                          id bigserial PRIMARY KEY,
                                          insert_time timestamp WITH TIME ZONE NOT NULL,
                                          version int,
                                          json jsonb NOT NULL
);
TRUNCATE TABLE TABLE_NAME;

CREATE INDEX TABLE_NAME_insert_time_idx ON TABLE_NAME USING btree (insert_time);
CREATE INDEX TABLE_NAME_exchange_id_idx ON TABLE_NAME USING btree (((json ->> 'exchange_id')::TEXT));
CREATE INDEX TABLE_NAME_currency_pair_idx ON TABLE_NAME USING btree (((json ->> 'currency_pair')::TEXT));

GRANT ALL PRIVILEGES ON TABLE TABLE_NAME TO dev;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO dev;