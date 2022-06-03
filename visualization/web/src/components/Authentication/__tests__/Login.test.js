import React from "react";
import { shallow } from "enzyme";
import Login from "../Login";

describe("Authentication/Login", () => {
  it("renders Login form", () => {
    const wrapper = shallow(<Login />);
    expect(wrapper).toMatchSnapshot();
  });
});
