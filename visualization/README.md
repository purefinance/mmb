#### 1. This application provides access to the liquidity data.

#### 2. It contains two modules:

#### [API](api)
This module provides data by getting from the database.
The API transmits real-time data to clients via WS (every N seconds by subscription) 
and also makes it possible to fetch data via HTTP

Based on actix, sqlx

Casbin is used for authentication.
Rules for route permissions are located in [api/policy/policy.csv](api/policy/policy.csv)
https://github.com/casbin/casbin-rs#how-it-works

Swagger UI: `{API_URL}/swagger-ui/index.html?url=/swagger-spec#/`

Swagger Spec: `{API_URL}/swagger-spec`

#### [Web](web)
Web application based on React. 
It connects to the API using the WS protocol and HTTP and get data from there.

#### 3. Manual Testing
Setup:
Configure `database_url` at [api/config/base.toml](api/config/base.toml)

```

# 1. Run api on localhost:53938
cd api
cargo run

# 2. Run webapp on localhost:3000
cd web
npm i
npm i -D
npm start

# Open web
http://localhost:3000/
login/pass: admin/admin

# Swagger 
http://127.0.0.1:53938/swagger-ui/index.html?url=/swagger-spec#/
http://127.0.0.1:53938/swagger-spec`
```
