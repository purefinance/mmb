#### 1. This application provides access to the liquidity data.

#### 2. It contains two modules:

#### [API](api)
This module provides data by getting from the database.
The API transmits real-time data to clients via WS (every N seconds by subscription) 
and also makes it possible to fetch data via HTTP

Based on actix, sqlx

#### [Web](web)
Web application based on React. 
It connects to the API using the WS protocol and HTTP and get data from there.

#### 3. Manual Testing

```

# 1. Run api on localhost:8080
cd api
cargo run

# 2. Run webapp on localhost:3000
cd web
npm install
npm start

# Open liquidity page
http://localhost:3000/liquidity/now/Aax/BTC-USD

```