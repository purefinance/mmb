import React, {Component} from "react";
import {Link} from "react-router-dom";
import Navigation from "../../components/Navbar/Navigation";
import {NavigationErrorBoundary} from "../../errorBoundaries";
import {Container, Col} from "react-bootstrap";
import "./Navbar.css";
import {getLogoImage} from "../../images";

class Navbar extends Component {
    constructor() {
        super();
        this.state = {open: false};

        this.onClick = this.onClick.bind(this);
        this.onClickClose = this.onClickClose.bind(this);
    }

    onClick() {
        this.setState({open: !this.state.open});
    }

    onClickClose() {
        this.setState({open: false});
    }

    render() {
        const {
            state: {interval, exchangeName, currencyCodePair},
        } = this.props.exchange;
        const {
            state: {clientDomain},
        } = this.props.app;

        return (
            <React.Fragment>
                <div id="slideElement" className={`slidePanel ${this.state.open ? "open" : ""}`}>
                    <Navigation
                        className="nav-menu open"
                        isSlidePanel
                        open={this.state.open}
                        onClickClose={this.onClickClose}
                        interval={interval}
                        exchangeName={exchangeName}
                        currencyCodePair={currencyCodePair}
                    />
                </div>

                <div
                    id="slideMenuButton"
                    className={`menu-button ${this.state.open ? "open" : ""}`}
                    onClick={this.onClick}>
                    <i className="fas fa-bars"></i>
                </div>

                <Container className="navbar base-container header">
                    <Col className="navbar-container">
                        <Link to="/" className="brand w-nav-brand">
                            {getLogoImage(clientDomain)}
                        </Link>

                        <NavigationErrorBoundary>
                            <Navigation
                                className="nav-menu"
                                open={false}
                                onClickClose={this.onClickClose}
                                interval={interval}
                                exchangeName={exchangeName}
                                currencyCodePair={currencyCodePair}
                            />
                        </NavigationErrorBoundary>
                    </Col>
                </Container>
            </React.Fragment>
        );
    }
}

export default Navbar;
