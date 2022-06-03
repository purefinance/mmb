import React, { Component } from "react";
import CryptolpAxios from "../../cryptolpaxios";
import Input from "../../containers/Validation/Input";
import { parseError, hasError } from "../../controls/validation";
import LaddaButton, { ZOOM_IN } from "react-ladda";

class Register extends Component {
  state = {
    username: "",
    email: "",
    password: "",
    confirmpassword: "",
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
      const registerResponse = await CryptolpAxios.register(this.state);
      await this.setState({ submitting: false });
      if (registerResponse.response && registerResponse.response.data.error) {
        const errors = parseError(registerResponse);
        await this.setState({ ...errors });
      } else window.location.href = "/login";
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
        id="email-form"
        name="email-form"
      >
        <Input
          type="text"
          value={this.state.username}
          onChange={this.handleChangeText}
          name="username"
          id="username"
          placeholder="Your Username"
          validationOption={{
            name: "UserName",
            required: true,
          }}
          setStateFunc={async (state) => await this.setState(state)}
          errorFields={this.state.errorFields}
        />

        <Input
          type="email"
          value={this.state.email}
          onChange={this.handleChangeText}
          name="email"
          id="email"
          placeholder="Your Email"
          validationOption={{
            name: "Email",
            required: true,
            email: true,
          }}
          setStateFunc={async (state) => await this.setState(state)}
          errorFields={this.state.errorFields}
        />

        <Input
          type="password"
          value={this.state.password}
          onChange={this.handleChangeText}
          name="password"
          id="password-2"
          placeholder="Password"
          validationOption={{
            name: "Password",
            required: true,
            minLength: {
              length: 6,
              errorMessage: "The Password must be at least 6 characters long.",
            },
          }}
          setStateFunc={async (state) => await this.setState(state)}
          errorFields={this.state.errorFields}
        />

        <Input
          type="password"
          value={this.state.confirmpassword}
          onChange={this.handleChangeText}
          name="confirmpassword"
          id="password-4"
          placeholder="Confirm Password"
          validationOption={{
            name: "ConfirmPassword",
            required: true,
            equal: {
              compare: this.state.password,
              errorMessage:
                "The password and confirmation password do not match.",
            },
          }}
          setStateFunc={async (state) => await this.setState(state)}
          errorFields={this.state.errorFields}
        />

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
          Sign Up
        </LaddaButton>
      </form>
    );
  }
}

export default Register;
