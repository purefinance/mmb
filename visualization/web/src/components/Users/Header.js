import React from "react";
import { Row, Col } from "react-bootstrap";

const Header = () => {
  return (
    <Row className="base-row top-div">
      <Col md={3} sm xs className="base-col title-user-screen">
        Name
      </Col>
      <Col md={3} sm xs className="base-col title-user-screen">
        Email
      </Col>
      <Col md={3} sm xs className="base-col title-user-screen">
        Creation Date
      </Col>
      <Col md={3} sm xs className="base-col title-user-screen">
        Role
      </Col>
    </Row>
  );
};

export default Header;
