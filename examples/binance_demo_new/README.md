It is just a testing project. For start project you should:
1. create `credentials.toml` in folder [src](./src/)
2. run docker-compose from root of repository
3. install [sqlx-cli](https://crates.io/crates/sqlx-cli) with postgres support if not installed
   ```
   cargo install sqlx-cli --no-default-features --features rustls,postgres
   ```
4. apply migrations from folder [binance_demo_new/migrations](./migrations) (working directory should be [binance_demo_new](.)):  
   ```
   sqlx migrate run
   ```
6. `cargo run` from folder [src](./src/)