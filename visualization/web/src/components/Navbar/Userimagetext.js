import React, { Component } from "react";
import CryptolpAxios from "../../cryptolpaxios";
import "../../Style.css";

import Dropdown from "../../controls/Dropdown/Dropdown";
import NavLink from "react-bootstrap/NavLink";

class Userimagetext extends Component {
  constructor() {
    super();
    this.state = { userInfo: CryptolpAxios.userInfo };
    CryptolpAxios.userUpdatedListners.push(this.updateUserInfo);
  }

  updateUserInfo = () => {
    this.forceUpdate();
    this.setState({ userInfo: CryptolpAxios.userInfo });
  };

  componentWillUnmount() {
    const index = CryptolpAxios.userUpdatedListners.indexOf(
      this.updateUserInfo
    );
    CryptolpAxios.userUpdatedListners.splice(index, 1);
  }

  render() {
    return !this.props.isSlidePanel ? (
      <Dropdown
        noArrow
        id="ExchangeDropdown"
        headerText="user-name-text-bold"
        value={this.state.userInfo.username}
        image={
          <img
            src="/images/man2x.png"
            height="36"
            width="36"
            alt=""
            className="profile-image"
          />
        }
      >
        <div
          onClick={CryptolpAxios.logout}
          className="dropdown-text-inner currency"
        >
          Log Out
        </div>
      </Dropdown>
    ) : (
      <NavLink
        onClick={CryptolpAxios.logout}
        className={`nav-link w-nav-link ${
          this.props.open ? "w--nav-link-open" : ""
        }`}
      >
        <strong>Log Out</strong>
      </NavLink>
    );
  }
}

export default Userimagetext;
