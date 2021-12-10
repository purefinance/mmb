## Welcome to mmb
Mmb is an open-source crypto trading engine implemented in Rust

## Connectors

![GREEN](https://via.placeholder.com/15/008000/?text=+) GREEN - Connector is working properly and safe to use

![YELLOW](https://via.placeholder.com/15/ffff00/?text=+) YELLOW - Connector is either new or has one or more issues

![RED](https://via.placeholder.com/15/f03c15/?text=+) RED - Connector is broken and unusable


| logo | id | name | ver | doc | status |
|:---:|:---:|:---:|:---:|:---:|:---:|
| <img src="assets/binance-logo.jpg" alt="Binance" width="90" /> | binance | [Binance](https://www.binance.com/) | 3 | [API](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md) | ![GREEN](https://via.placeholder.com/15/008000/?text=+)|

## Quick Start

1. Go to `src` directory
2. Configure your strategy in `config.toml`
3. Provide api keys and secrets in `credentials.toml`
```
[Binance_0]
api_key = "..."
secret_key = "..."
```
4. Execute `cargo build`
5. Execute `cargo run`

## Contributions

We welcome contributions from the community:
- **Code and documentation contributions** via [pull requests](https://github.com/purefinance/mmb/pulls)
- **Bug reports and feature requests** through [Github issues](https://github.com/purefinance/mmb/issues)
