import React, { Component } from "react";
import { Route, Switch, BrowserRouter, Redirect } from "react-router-dom";
import { Subscribe } from "unstated";

import CryptolpAxios from "./cryptolpaxios";
import { GlobalErrorBoundary } from "./errorBoundaries";
import constants from "./constants.js";

import Liquidity from "./containers/Liquidity";
import Balances from "./containers/Balances";
import Users from "./containers/Users";
import Volume from "./containers/Volume";
import TradeSignals from "./containers/TradeSignals";

import ExchangeContainer from "./unstatedContainers/ExchangeContainer";
import LiquidityContainer from "./unstatedContainers/LiquidityContainer";
import UserListContainer from "./unstatedContainers/UserListContainer";
import VolumeContainer from "./unstatedContainers/VolumeContainer";
import SignalsContainer from "./unstatedContainers/SignalsContainer";
import ExplanationContainer from "./unstatedContainers/ExplanationContainer";

import AuthenticationContainer from "./components/Authentication/AuthenticationContainer";
import Navbar from "./containers/Navbar";
import Price from "./components/Price/Price";

import "./Style.css";
// import Dashboard from "./containers/Dashboard";
import Header from "./containers/Header";
import WsContainer from "./unstatedContainers/WsContainer";
import Signals from "./containers/Signals";
import PL from "./containers/PL";
import Configuration from "./containers/Configuration";
import ConfigurationContainer from "./unstatedContainers/ConfigurationContainer";
import PostponedFills from "./containers/PostponedFills";
import PostponedFillsContainer from "./unstatedContainers/PostponedFillsContainer";
import LiquidityNow from "./containers/LiquidityNow";
import Explanation from "./containers/Explanation";
import AppContainer from "./unstatedContainers/AppContainer";
import PLGraphContainer from "./unstatedContainers/PLGraphContainer";
import PLGraph from "./containers/PLGraph";
import { ToastContainer } from "react-toastify";

import "react-toastify/dist/ReactToastify.css";

class App extends Component {
  constructor() {
    super();
    CryptolpAxios.loadToken();

    this.state = {
      isAuthorized: CryptolpAxios.isAuthorized,
      role: "",
      priceIndicators: {},
    };

    CryptolpAxios.userUpdatedListners.push(() => {
      this.setState({
        isAuthorized: CryptolpAxios.isAuthorized,
        role: CryptolpAxios.role,
      });
    });
  }

  render() {
    const isArbitrage =
      CryptolpAxios.clientType === constants.clientType.arbitrage;
    const isIco = CryptolpAxios.clientType === constants.clientType.ico;
    const isSignals = CryptolpAxios.clientType === constants.clientType.signals;

    const isAdmin =
      CryptolpAxios.role &&
      CryptolpAxios.role.toUpperCase() === constants.clientRoles.admin;
    const isSuperAdmin =
      CryptolpAxios.role &&
      CryptolpAxios.role.toUpperCase() === constants.clientRoles.superAdmin;

    let routes = null;
    let headers = null;
    if (this.state.isAuthorized) {
      routes = (
        <Switch>
          {isAdmin && (
            <Route
              exact
              path="/users"
              render={(props) => (
                <Subscribe to={[UserListContainer]}>
                  {(userList) => <Users {...props} userList={userList} />}
                </Subscribe>
              )}
            />
          )}

          {isSuperAdmin && (
            <Route
              exact
              path="/configuration"
              render={() => (
                <Subscribe to={[ConfigurationContainer]}>
                  {(configuration) => (
                    <Configuration configuration={configuration} />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isSuperAdmin && (
            <Route
              exact
              path="/postponed-fills"
              render={() => (
                <Subscribe to={[PostponedFillsContainer]}>
                  {(postponedFills) => (
                    <PostponedFills postponedFills={postponedFills} />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isSuperAdmin && (
            <Route
              exact
              path="/explanation/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer, ExplanationContainer]}>
                  {(exchange, explanation) => (
                    <Explanation
                      exchange={exchange}
                      explanation={explanation}
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isSuperAdmin && (
            <Route
              exact
              path="/plgraph/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer, PLGraphContainer]}>
                  {(exchange, data) => (
                    <PLGraph exchange={exchange} data={data} />
                  )}
                </Subscribe>
              )}
            />
          )}

          {!isArbitrage && (
            <Route
              exact
              path="/volume/:interval?/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe
                  to={[ExchangeContainer, VolumeContainer, WsContainer]}
                >
                  {(exchange, volume, ws) => (
                    <Volume exchange={exchange} volume={volume} ws={ws} />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isIco && (
            <Route
              exact
              path="/price"
              render={(props) => (
                <Price
                  {...props}
                  preprocessedOrderBook={this.state.preprocessedOrderBook}
                  priceIndicators={this.state.priceIndicators}
                  interval={this.state.interval}
                />
              )}
            />
          )}

          {isSignals && (
            <Route
              exact
              path="/signals/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer, SignalsContainer]}>
                  {(exchange, signals) => (
                    <Signals exchange={exchange} signals={signals} />
                  )}
                </Subscribe>
              )}
            />
          )}

          {!isArbitrage && (
            <Route
              exact
              path="/liquidity/:interval?/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe
                  to={[ExchangeContainer, LiquidityContainer, WsContainer]}
                >
                  {(exchange, liquidity, ws) => (
                    <Liquidity
                      exchange={exchange}
                      liquidity={liquidity}
                      ws={ws}
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          <Route
            exact
            path="/rebalancing"
            render={() => (
              <Subscribe to={[WsContainer]}>
                {(ws) => <Balances ws={ws} />}
              </Subscribe>
            )}
          />

          <Route
            exact
            path="/trade-signals"
            render={() => (
              <Subscribe to={[WsContainer]}>
                {(ws) => <TradeSignals ws={ws} />}
              </Subscribe>
            )}
          />

          <Route
            exact
            path="/profits/:exchangeName?/:urlCurrencyCodePair?"
            render={() => (
              <Subscribe to={[ExchangeContainer, WsContainer]}>
                {(exchange, ws) => <PL exchange={exchange} ws={ws} />}
              </Subscribe>
            )}
          />

          {isArbitrage && (
            <Route
              exact
              path="/arbitrage/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer, WsContainer]}>
                  {(exchange, ws) => (
                    <LiquidityNow exchange={exchange} ws={ws} />
                  )}
                </Subscribe>
              )}
            />
          )}

          {!isArbitrage && (
            <Route
              exact
              path="/"
              render={() => {
                window.location.href = "/liquidity/now/";
              }}
              // !TODO uncomment it if you want to enable Dashboard
              // render={() => (
              //   <Subscribe to={[WsContainer]}>
              //     {(ws) => <Dashboard ws={ws} />}
              //   </Subscribe>
              // )}
            />
          )}

          <Redirect to={!isArbitrage ? "/" : "/arbitrage"} />
        </Switch>
      );

      headers = (
        <Switch>
          {!isArbitrage && (
            <Route
              exact
              path="/volume/:interval?/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer]}>
                  {(exchange) => (
                    <Header
                      path="/volume"
                      exchange={exchange}
                      isNeedInterval
                      isNeedExchange
                      isNeedCurrencyCodePair
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          {!isArbitrage && (
            <Route
              exact
              path="/liquidity/:interval?/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer]}>
                  {(exchange) => (
                    <Header
                      path="/liquidity"
                      exchange={exchange}
                      isNeedInterval
                      isNeedExchange
                      isNeedCurrencyCodePair
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isSuperAdmin && (
            <Route
              exact
              path="/explanation/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer]}>
                  {(exchange) => (
                    <Header
                      path="/explanation"
                      exchange={exchange}
                      isNeedExchange
                      isNeedCurrencyCodePair
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isSuperAdmin && (
            <Route
              exact
              path="/plgraph/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer]}>
                  {(exchange) => (
                    <Header
                      path="/plgraph"
                      exchange={exchange}
                      isNeedExchange
                      isNeedCurrencyCodePair
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isArbitrage && (
            <Route
              exact
              path="/arbitrage/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer]}>
                  {(exchange) => (
                    <Header
                      path="/arbitrage"
                      exchange={exchange}
                      isNeedExchange
                      isNeedCurrencyCodePair
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          {isSignals && (
            <Route
              exact
              path="/signals/:exchangeName?/:urlCurrencyCodePair?"
              render={() => (
                <Subscribe to={[ExchangeContainer]}>
                  {(exchange) => (
                    <Header
                      path="/signals"
                      exchange={exchange}
                      isNeedExchange
                      isNeedCurrencyCodePair
                    />
                  )}
                </Subscribe>
              )}
            />
          )}

          <Route
            exact
            path="/profits/:exchangeName?/:urlCurrencyCodePair?"
            render={() => (
              <Subscribe to={[ExchangeContainer]}>
                {(exchange) => (
                  <Header
                    needAll
                    path="/profits"
                    exchange={exchange}
                    isNeedExchange
                    isNeedCurrencyCodePair
                  />
                )}
              </Subscribe>
            )}
          />
        </Switch>
      );
    }

    return (
      <GlobalErrorBoundary>
        <ToastContainer
          position="top-right"
          autoClose={5000}
          hideProgressBar={true}
          newestOnTop={false}
          closeOnClick
          rtl={false}
          pauseOnFocusLoss
          draggable
          pauseOnHover
          limit={1}
        />
        <BrowserRouter>
          {this.state.isAuthorized ? (
            <React.Fragment>
              <Subscribe to={[AppContainer, ExchangeContainer]}>
                {(app, exchange) => <Navbar app={app} exchange={exchange} />}
              </Subscribe>
              {headers}
              {routes}
            </React.Fragment>
          ) : (
            <Subscribe to={[AppContainer]}>
              {(app) => <AuthenticationContainer app={app} />}
            </Subscribe>
          )}
        </BrowserRouter>
      </GlobalErrorBoundary>
    );
  }
}

export default App;
