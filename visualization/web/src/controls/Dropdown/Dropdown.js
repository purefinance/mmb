import React from "react";
import { Dropdown, Row } from "react-bootstrap";
import "./Dropdown.css";

const CustomToggle = React.forwardRef(
  ({ children, onClick, iconClassName, arrowStyle }) => (
    <Row className="base-row" style={{ cursor: "pointer" }} onClick={onClick}>
      {children}
      <i
        className={`fas fa-angle-down ${
          iconClassName ? iconClassName : ""
        } ${arrowStyle}`}
      ></i>
    </Row>
  )
);

const CustomMenu = React.forwardRef(
  // eslint-disable-next-line no-unused-vars
  ({ children, style, className, "aria-labelledby": labeledBy }, ref) => (
    <div className={className}>{children}</div>
  )
);

class MyDropdown extends React.Component {
  render() {
    let children = {};

    if (this.props.children && this.props.children.length > 1)
      children = this.props.children.map((ch, index) => {
        if (ch.type.displayName === "NavLink") {
          return (
            <Dropdown.Item
              id="element-for-select"
              className={ch.props.className}
              key={index}
              onClick={() => ch.props.onClick()}
            >
              {ch.props.children}
            </Dropdown.Item>
          );
        }

        return (
          <Dropdown.Item
            id="element-for-select"
            key={index}
            onClick={() => ch.props.onClick()}
          >
            {ch}
          </Dropdown.Item>
        );
      });
    else {
      children = (
        <Dropdown.Item
          id="element-for-select"
          onClick={
            this.props.children &&
            this.props.children.props &&
            this.props.children.props.onClick
          }
        >
          {this.props.children}
        </Dropdown.Item>
      );
    }

    return (
      <Dropdown id={this.props.id} className={this.props.className}>
        <Dropdown.Toggle
          id={this.props.id}
          as={CustomToggle}
          arrowStyle={`icon-arrow ${this.props.user ? "user" : ""} ${
            this.props.noArrow ? " noArrow" : ""
          }`}
          iconClassName={this.props.iconClassName}
        >
          {this.props.image}
          <div className={this.props.headerText}>{this.props.value}</div>
        </Dropdown.Toggle>

        <Dropdown.Menu as={CustomMenu}>{children}</Dropdown.Menu>
      </Dropdown>
    );
  }
}

export default MyDropdown;
