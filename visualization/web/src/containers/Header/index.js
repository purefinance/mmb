import React from "react";
import {withRouter} from "react-router-dom";

import Exchanges from "../../components/Header/Exchanges";
import Symbols from "../../components/Header/Symbols";
import Timewrapper from "../../components/Header/Timewrapper";

import {HeaderErrorBoundary} from "../../errorBoundaries";
import {Container, Col, Row} from "react-bootstrap";
import Strategies from "../../components/Header/Strategies";
import utils from "../../utils";

class Header extends React.Component {
    async componentDidMount() {
        await this.update();
    }

    async componentDidUpdate() {
        await this.update();
    }

    async update() {
        await utils.checkParameters(this.props);
    }

    render() {
        return (
            <HeaderErrorBoundary>
                <Container className="base-container">
                    <Row className="header-content base-row">
                        <Col
                            md={!this.props.needShowStrategies ? 6 : 10}
                            sm={!this.props.needShowStrategies ? 7 : 12}
                            xs={12}>
                            <Row className="base-row">
                                <Col className="base-col">
                                    <Exchanges {...this.props} />
                                </Col>
                                <Col className="base-col">
                                    <Symbols {...this.props} />
                                </Col>
                                {this.props.needShowStrategies && (
                                    <Col className="base-col">
                                        <Strategies {...this.props} />
                                    </Col>
                                )}
                            </Row>
                        </Col>

                        {this.props.needTimeWrapper && (
                            <Timewrapper
                                path={this.props.path}
                                hideNow={this.props.hideNow}
                                interval={this.props.exchange.state.interval}
                                currencyPair={this.props.exchange.state.currencyPair}
                                exchangeName={this.props.exchange.state.exchangeName}
                            />
                        )}
                    </Row>
                </Container>
            </HeaderErrorBoundary>
        );
    }
}

export default withRouter(Header);
