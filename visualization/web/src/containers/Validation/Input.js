import React, {Component} from "react";
import validator from "validator";

class Input extends Component {
    state = {
        errorMessage: "",
    };

    constructor(prop) {
        super(prop);

        this.validate = this.validate.bind(this);
        this.onChange = this.onChange.bind(this);
    }

    async validate(value) {
        const {validationOption} = this.props;

        let errorMessage = "";
        if (validationOption.required && !value.length) errorMessage = " cannot be empty.";
        if (validationOption.email && !validator.isEmail(value)) {
            errorMessage = value + " is not a valid email address";
        }
        if (validationOption.minLength && value.length < validationOption.minLength.length) {
            errorMessage = validationOption.minLength.errorMessage;
        }
        if (validationOption.equal && value !== validationOption.equal.compare) {
            errorMessage = validationOption.equal.errorMessage;
        }

        let errorVals = this.props.errorFields;
        errorVals[validationOption.name] = errorMessage;
        await this.props.setStateFunc({errorFields: errorVals});
        await this.setState({errorMessage});
    }

    onChange(e) {
        const value = e.target.value;
        this.validate(value);
        this.props.onChange(this.props.name, value);
    }

    render() {
        const {id, type, name, value, tabIndex, placeholder} = this.props;

        return (
            <React.Fragment>
                <input
                    id={id}
                    name={name}
                    type={type}
                    value={value}
                    tabIndex={tabIndex}
                    maxLength={256}
                    onChange={this.onChange}
                    placeholder={placeholder}
                    className={this.state.errorMessage ? "text-field text-field-error" : "text-field"}
                />
                {this.state.errorMessage && <label className="error-text">{this.state.errorMessage}</label>}
            </React.Fragment>
        );
    }
}

export default Input;
