import React, { Component } from "react";
import Userimagetext from "./Userimagetext";
import { NavLink, withRouter } from "react-router-dom";
import CryptolpAxios from "../../cryptolpaxios";
import constants from "../../constants.js";
import { Nav } from "react-bootstrap";
import MoreLink from "./MoreLink";
import utils from "../../utils";

class Navdropdown extends Component {
  constructor() {
    super();
    this.state = {
      isAuthorized: CryptolpAxios.isAuthorized,
      role: CryptolpAxios.role,
    };
    CryptolpAxios.userUpdatedListners.push(this.updateIsAuthorized);
  }

  updateIsAuthorized = () =>
    this.setState({
      isAuthorized: CryptolpAxios.isAuthorized,
      role: CryptolpAxios.role,
    });

  componentWillUnmount() {
    const index = CryptolpAxios.userUpdatedListners.indexOf(
      this.updateIsAuthorized
    );
    CryptolpAxios.userUpdatedListners.splice(index, 1);
  }

  getNavLink(exact, path, innerText, isDropdown, key) {
    return (
      <NavLink
        key={key}
        exact={exact}
        to={path}
        onClick={(e) => {
          if (isDropdown) utils.pushNewLinkToHistory(this.props.history, path);
          this.props.onClickClose(e);
        }}
        className={`nav-link ${
          isDropdown && !this.props.isSlidePanel ? "dropdown" : ""
        }`}
        activeClassName={`active-button-menu`}
      >
        {innerText}
      </NavLink>
    );
  }

  navArbitrage = (isDropdown, key) =>
    this.getNavLink(
      true,
      `/arbitrage/${this.props.exchangeName}/${utils.urlCodePair(
        this.props.currencyCodePair
      )}`,
      "Arbitrage",
      isDropdown,
      key
    );
  navDashboard = (isDropdown, key) =>
    this.getNavLink(true, "/", "Dashboard", isDropdown, key);
  navLiquidity = (isDropdown, key) =>
    this.getNavLink(
      false,
      `/liquidity/now/${this.props.exchangeName}/${utils.urlCodePair(
        this.props.currencyCodePair
      )}`,
      "Liquidity",
      isDropdown,
      key
    );
  navVolume = (isDropdown, key) =>
    this.getNavLink(
      false,
      `/volume/now/${this.props.exchangeName}/${utils.urlCodePair(
        this.props.currencyCodePair
      )}`,
      "Volume",
      isDropdown,
      key
    );
  navPL = (isDropdown, key) =>
    this.getNavLink(
      true,
      `/profits/${this.props.exchangeName}/${utils.urlCodePair(
        this.props.currencyCodePair
      )}`,
      "Profit Loss",
      isDropdown,
      key
    );
  navBalances = (isDropdown, key) =>
    this.getNavLink(true, "/rebalancing", "Balances", isDropdown, key);
  navSignals = (isDropdown, key) =>
    this.getNavLink(
      true,
      `/signals/${this.props.exchangeName}/${utils.urlCodePair(
        this.props.currencyCodePair
      )}`,
      "Signals",
      isDropdown,
      key
    );
  navPrice = (isDropdown, key) =>
    this.getNavLink(true, "/price", "Price", isDropdown, key);
  navUsers = (isDropdown, key) =>
    this.getNavLink(true, "/users", "Users", isDropdown, key);
  navConfigurations = (isDropdown, key) =>
    this.getNavLink(true, "/configuration", "Configuration", isDropdown, key);
  navExplanation = (isDropdown, key) =>
    this.getNavLink(true, "/explanation", "Explanation", isDropdown, key);
  navPLGraph = (isDropdown, key) =>
    this.getNavLink(
      true,
      `/plgraph/${this.props.exchangeName}/${utils.urlCodePair(
        this.props.currencyCodePair
      )}`,
      "ProfitLoss Graph",
      isDropdown,
      key
    );
  navTradeSignals = (isDropdown, key) =>
    this.getNavLink(true, "/trade-signals", "Trade Signals", isDropdown, key);
  navPostponedFills = (isDropdown, key) =>
    this.getNavLink(
      true,
      "/postponed-fills",
      "Postponed Fills",
      isDropdown,
      key
    );

  render() {
    if (this.state.isAuthorized) {
      const navElements = [];
      if (CryptolpAxios.clientType === constants.clientType.arbitrage)
        navElements.push(this.navArbitrage);
      else {
        // !TODO uncomment these comments if you want to enable Nav buttons
        // navElements.push(this.navDashboard);
        navElements.push(this.navLiquidity);
        // navElements.push(this.navVolume);
      }
      // navElements.push(this.navPL);
      navElements.push(this.navBalances);
      // navElements.push(this.navTradeSignals);
      if (CryptolpAxios.clientType === constants.clientType.signals)
        navElements.push(this.navSignals);
      if (CryptolpAxios.clientType === constants.clientType.ico)
        navElements.push(this.navPrice);
      if (CryptolpAxios.role.toUpperCase() === constants.clientRoles.user) {
        navElements.push(this.navUsers);
      }
      if (CryptolpAxios.role.toUpperCase() === constants.clientRoles.admin) {
        navElements.push(this.navConfigurations);
        navElements.push(this.navExplanation);
        // navElements.push(this.navPostponedFills);
        // navElements.push(this.navPLGraph);
      }
      const navDropdown = [];
      // First time we take 2 elements for general length navMenu 5
      if (navElements.length > 5)
        while (navElements.length >= 5) {
          navDropdown.push(navElements.pop());
        }
      navDropdown.reverse();

      return (
        <Nav className={this.props.className}>
          {navElements.map((el, index) => el(false, index))}

          {navDropdown.length !== 0 && (
            <MoreLink isSlidePanel={this.props.isSlidePanel}>
              {navDropdown.map((el, index) => el(true, index))}
            </MoreLink>
          )}

          <Userimagetext isSlidePanel={this.props.isSlidePanel} />
        </Nav>
      );
    } else {
      return (
        <Nav className={this.props.className}>
          {this.getNavLink(true, "/login", "Login")}
          {this.getNavLink(true, "/register", "Register")}
        </Nav>
      );
    }
  }
}

export default withRouter(Navdropdown);
