# mmb Design Specification

## Short-term Development Goal

Create core engine and modular structure to enable connection to multiple order books and multiple exchanges.

## User Stories aka Outcomes sought aka Long-term Goals

Language:  Use modern, stable Rust for core program.  Use Rust for scripts, as a 1st choice, when possible.

Control Plane:  User controls interactions via RESTful API.

Multi-exchange support:  MVP is a single exchange support.  Future developers can author exchange modules, to expand the system.

Multi-strategy support:  MVP is a single strategy (akin to hummingbot's pure_market_making).  Future developers can author strategy modules, to expand the system.

Order book updates:  (Perhaps, states the obvious)   Program must be able to receive order book full-update and delta-update, either via polling (usually HTTPS/JSON) or streaming (usually WebSocket)

Asset tracking model:  (Perhaps, states the obvious)   Program requires some method of tracking capital in its various states:  in flight (waiting for blockchain confirmations), at exchange, inside exchange's trading engine, and in-orders.  It is likely that capital must be allocated to specific Strategy Plans or Buckets, to virtually separate capital as strategies execute in parallel.

