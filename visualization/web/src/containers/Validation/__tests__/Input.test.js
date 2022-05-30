import React from "react";
import {mount, shallow} from "enzyme";
import Input from "../Input";

function setStateFunc() {}

function onChange() {}

const errorLabel = <label className="error-text">UserName cannot be empty.</label>;

describe("containers/Validation/Login", () => {
    it("render control validation/Input", () => {
        const input = (
            <Input
                tabIndex="1"
                type="text"
                value=""
                id="UserName"
                name="UserName"
                errorFields={[]}
                setStateFunc={setStateFunc}
                onChange={onChange}
                validationOption={{check: false}}
            />
        );

        const wrapper = shallow(input);
        expect(wrapper).toMatchSnapshot();

        const wMount = mount(input);
        expect(wMount).toMatchSnapshot();
    });

    it("render control validation/Input with error (required)", () => {
        let errorFields = [];
        errorFields["UserName"] = "UserName cannot be empty.";
        const wrapper = mount(
            <Input
                tabIndex="1"
                type="text"
                value=""
                id="UserName"
                name="UserName"
                errorFields={errorFields}
                setStateFunc={setStateFunc}
                onChange={onChange}
                validationOption={{check: true, required: true, name: "UserName"}}
            />,
        );
        expect(wrapper).toMatchSnapshot();
        expect(wrapper.contains(errorLabel)).toEqual(true);
    });
});
