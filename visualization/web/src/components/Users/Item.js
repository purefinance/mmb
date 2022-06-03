import React from "react";
import PropTypes from "prop-types";
import LaddaButton, { ZOOM_IN } from "react-ladda";
import Dropdown from "../../controls/Dropdown/Dropdown";
import { Row, Col } from "react-bootstrap";

class Item extends React.Component {
  constructor(props) {
    super(props);
    this.state = { submitting: false };
  }
  saveUser = (user) => {
    this.setState({ submitting: true }, async () => {
      try {
        await this.props.saveUser(user);
        this.setState({ submitting: false });
      } catch (e) {
        console.error("Error", e);
      }
    });
  };

  render() {
    const { user, roles, updateRole } = this.props;
    const role = roles.find((role) => role.toUpperCase() === user.role)
      ? roles.find((role) => role.toUpperCase() === user.role)
      : roles[0];

    return (
      <Row className="base-row user-data-row">
        <Col md={3} sm xs className="base-col user-text name">
          <div>{user.userName}</div>
        </Col>
        <Col md={3} sm xs className="base-col user-text">
          <div>{user.email}</div>
        </Col>
        <Col md={3} sm xs className="base-col user-text">
          <div className="text-block-12">
            {new Date(user.creationDate).toDateString("mm/dd/yyyy")}
          </div>
        </Col>
        <Col md={3} sm xs className="base-col">
          <Row className="base-row">
            <Col md={6} sm xs>
              <Dropdown
                user
                value={role}
                onUpdate={updateRole}
                doNotOpenOnHover={true}
                headerText="user-role-text"
              >
                {roles.map((role) => (
                  <div
                    key={role.toUpperCase()}
                    className="dropdown-link-2 dropdown-link w-dropdown-link"
                    onClick={() => updateRole(user.id, role.toUpperCase())}
                  >
                    {role}{" "}
                  </div>
                ))}
              </Dropdown>
            </Col>
            <Col md={6} sm xs>
              <LaddaButton
                loading={this.state.submitting}
                onClick={() => this.saveUser(user)}
                data-color="#f88710"
                data-style={ZOOM_IN}
                data-spinner-size={30}
                data-spinner-color="#ffffff"
                data-spinner-lines={8}
                className="custom-LadaButton"
              >
                Update
              </LaddaButton>
            </Col>
          </Row>
        </Col>
      </Row>
    );
  }
}

Item.propTypes = {
  user: PropTypes.object.isRequired,
  roles: PropTypes.array.isRequired,
  updateRole: PropTypes.func.isRequired,
  saveUser: PropTypes.func.isRequired,
};

export default Item;
