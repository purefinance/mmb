import React, {Component} from "react";
import {Route, Switch, Redirect, NavLink, Link} from "react-router-dom";
import Login from "./Login";
import Register from "./Register";
import {Container, Row} from "react-bootstrap";
import "./Authentication.css";
import {getLogoImage} from "../../images";

class AuthenticationContainer extends Component {
    render() {
        const {
            state: {clientDomain},
        } = this.props.app;
        return (
            <Container className="base-container">
                <Row className="navbar-container center base-row">
                    <Link to="/" className="brand">
                        {getLogoImage(clientDomain)}
                    </Link>
                </Row>
                <Container className="base-container authentication">
                    <Row className="tab-menu">
                        <NavLink to="/login" exact className="auth-tab" activeClassName="current">
                            Log In
                        </NavLink>
                        <NavLink to="/signup" exact className="auth-tab" activeClassName="current">
                            Sign Up
                        </NavLink>
                    </Row>
                    <Switch>
                        <Route exact path="/login" component={Login} />
                        <Route exact path="/signup" component={Register} />
                        <Redirect to="/login" />
                    </Switch>
                </Container>
            </Container>
        );
    }
}

export default AuthenticationContainer;
