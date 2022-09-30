create table bot_sessions
(
	id text not null
		constraint sessions_pk
			primary key,
	datetime_from timestamp default now() not null,
	datetime_to timestamp default now() not null
);