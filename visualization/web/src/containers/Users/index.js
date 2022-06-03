import React, { Component } from "react";
import { Header, List } from "../../components/Users";
import { BodyErrorBoundary } from "../../errorBoundaries";
import { Container } from "react-bootstrap";
import "./index.css";

class Users extends Component {
  componentDidMount() {
    this.props.userList.loadUserList();
  }
  render() {
    return (
      <Container className="base-container">
        <Header />
        <BodyErrorBoundary>
          <List userList={this.props.userList} />
        </BodyErrorBoundary>
      </Container>
    );
  }
}

export default Users;
