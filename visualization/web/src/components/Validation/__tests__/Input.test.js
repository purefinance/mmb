import React from "react";
import {shallow} from "enzyme";
import Input from "../Input";

function onChange() {}

const errorLabel = <label className="error-text">UserName cannot be empty.</label>;

describe("components/Validation/Input", () => {
    it("render control validation/Input", () => {
        const wrapper = shallow(
            <Input tabIndex="1" type="text" value="" id="UserName" name="UserName" onChange={onChange} error="" />,
        );
        expect(wrapper).toMatchSnapshot();
        expect(wrapper.contains(errorLabel)).toEqual(false);
        expect(wrapper.find("#UserName").hasClass("text-field-error")).toEqual(false);
    });

    it("render control validation/Input with error (required)", () => {
        let errorFields = [];
        errorFields["UserName"] = "UserName cannot be empty.";
        const wrapper = shallow(
            <Input
                tabIndex="1"
                type="text"
                value=""
                id="UserName"
                name="UserName"
                onChange={onChange}
                error="UserName cannot be empty."
            />,
        );
        expect(wrapper).toMatchSnapshot();
        expect(wrapper.contains(errorLabel)).toEqual(true);
        expect(wrapper.find("#UserName").hasClass("text-field-error")).toEqual(true);
    });

    it("render control validation/Input with value", () => {
        let errorFields = [];
        errorFields["UserName"] = "UserName cannot be empty.";
        const wrapper = shallow(
            <Input
                tabIndex="1"
                type="text"
                value="Test value"
                id="UserName"
                name="UserName"
                onChange={onChange}
                error=""
            />,
        );
        expect(wrapper).toMatchSnapshot();
        expect(wrapper.contains(errorLabel)).toEqual(false);
        expect(wrapper.find("#UserName").hasClass("text-field-error")).toEqual(false);
        expect(wrapper.find("#UserName").prop("value")).toEqual("Test value");
    });
});
