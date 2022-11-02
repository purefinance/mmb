create table cleanup_settings
(
	table_name text not null,
	column_name text not null,
	period interval not null
);

create unique index cleanup_settings_table_name_uindex
	on cleanup_settings (table_name);

insert into public.cleanup_settings (table_name, period, column_name)
values  ('balance_updates', '1 mons', 'insert_time'),
        ('balances', '1 mons', 'insert_time'),
        ('disposition_explanations', '1 mons', 'insert_time'),
        ('liquidation_prices', '1 mons', 'insert_time'),
        ('liquidity_order_books', '1 mons', 'insert_time'),
        ('orders', '3 mons', 'insert_time'),
        ('prices_sources', '1 mons', 'insert_time'),
        ('profit_loss_balance_changes', '1 mons', 'insert_time'),
        ('trades_events', '1 mons', 'insert_time');