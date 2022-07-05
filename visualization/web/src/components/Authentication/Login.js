import React, { Component } from "react";
import CryptolpAxios from "../../cryptolpaxios";
import { parseError, hasError } from "../../controls/validation";
import Input from "../../containers/Validation/Input";
import LaddaButton, { ZOOM_IN } from "react-ladda";
import { Row } from "react-bootstrap";

class Login extends Component {
  state = {
    username: "",
    password: "",
    rememberme: false,
    submitting: false,
    error: "",
    errorFields: [],
  };

  constructor(prop) {
    super(prop);

    this.submit = this.submit.bind(this);
    this.validateForm = this.validateForm.bind(this);
    this.handleChangeText = this.handleChangeText.bind(this);
  }

  async validateForm(e) {
    e.preventDefault();
    if (!hasError(this.state.errorFields)) {
      await this.setState({ submitting: true, error: "", errorFields: [] });
      await this.submit();
    }
  }

  async submit() {
    try {
      const loginResponse = await CryptolpAxios.login(this.state);
      await this.setState({ submitting: false });
      if (loginResponse.data && loginResponse.data.error) {
        const errors = parseError(loginResponse);
        await this.setState({ ...errors });
      } else {
        try {
          const clientType = await CryptolpAxios.getClientType();
          CryptolpAxios.setToken(loginResponse.data, clientType);
        } catch (error) {
          console.log(error);
        }
      }
    } catch (error) {
      await this.setState({ submitting: false });
      console.log(error);
    }
  }

  async handleChangeText(id, value) {
    await this.setState({ [id]: value });
  }

  render() {
    return (
      <form
        className="form"
        onSubmit={this.validateForm}
        id="login-form"
        name="login-form"
      >
        <Input
          type="text"
          value={this.state.username}
          onChange={this.handleChangeText}
          name="username"
          id="username"
          placeholder="Your UserName"
          validationOption={{
            name: "UserName",
            required: true,
          }}
          setStateFunc={async (state) => await this.setState(state)}
          errorFields={this.state.errorFields}
        />

        <Input
          type="password"
          value={this.state.password}
          onChange={this.handleChangeText}
          name="password"
          id="password"
          placeholder="Password"
          validationOption={{
            name: "Password",
            required: true,
          }}
          setStateFunc={async (state) => await this.setState(state)}
          errorFields={this.state.errorFields}
        />

        <Row className="base-row">
          <input
            type="checkbox"
            className="remember-checkbox"
            onChange={async (e) =>
              await this.setState({ rememberme: e.target.checked })
            }
            name="rememberme"
            id="rememberme"
          />
          <label htmlFor="rememberme" className="remember-lable">
            Remember me
          </label>
        </Row>
        {this.state.error && (
          <div className="form-fail" style={{ display: "block" }}>
            <div>{this.state.error}</div>
          </div>
        )}

        <LaddaButton
          loading={this.state.submitting}
          onClick={this.validateForm}
          data-color="#f88710"
          data-style={ZOOM_IN}
          data-spinner-size={30}
          data-spinner-color="#ffffff"
          data-spinner-lines={12}
          className="submit-button w-button"
        >
          Log In
        </LaddaButton>
      </form>
    );
  }
}

export default Login;
