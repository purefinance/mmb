import React, { Component } from "react";
import PropTypes from "prop-types";
import "./validation.css";

class Input extends Component {
  render() {
    const {
      error,
      id,
      maxLength,
      name,
      onChange,
      placeholder,
      tabIndex,
      type,
      value,
    } = this.props;
    return (
      <React.Fragment>
        <input
          id={id}
          type={type}
          value={value}
          tabIndex={tabIndex}
          onChange={onChange}
          className={
            error ? "text-field w-input text-field-error" : "text-field w-input"
          }
          maxLength={maxLength}
          name={name}
          placeholder={placeholder}
        />
        {error && <label className="error-text">{error}</label>}
      </React.Fragment>
    );
  }
}

Input.propTypes = {
  id: PropTypes.string.isRequired,
  maxLength: PropTypes.string,
  name: PropTypes.string.isRequired,
  onChange: PropTypes.func.isRequired,
  error: PropTypes.string,
  placeholder: PropTypes.string,
  tabIndex: PropTypes.string.isRequired,
  type: PropTypes.string.isRequired,
  value: PropTypes.string.isRequired,
};

export default Input;
