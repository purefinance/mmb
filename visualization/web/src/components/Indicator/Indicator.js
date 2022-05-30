import React, {Component} from "react";
import {Container, Row, Col, Tooltip, OverlayTrigger} from "react-bootstrap";
import "./Indicator.css";

class Indicator extends Component {
    getIndicatorText() {
        if (this.props.data === null || this.props.data === undefined) return "--";

        let text = this.props.prefix ? this.props.prefix : "";
        text += this.props.isColored && this.props.data > 0 ? "+ " : "";
        text += this.props.data;
        text += this.props.postfix ? " " + this.props.postfix : "";

        return text;
    }

    renderTooltip(tooltip) {
        return <Tooltip>{tooltip}</Tooltip>;
    }

    render() {
        const container = (
            <Container
                className={`base-background indicator-base ${this.props.percentage !== undefined ? "percent" : ""}`}>
                <Row
                    className={`indicator-text ${
                        this.props.isColored ? (this.props.data > 0 ? "green-text" : "red-text") : ""
                    }`}>
                    {this.getIndicatorText()}
                </Row>

                <Row className="indicator-title">
                    <Col>{this.props.title}</Col>
                    {this.props.percentage !== undefined && (
                        <div className={`indicator-numbers _${this.props.percentage.toString().length}`}>
                            {this.props.percentage + "%"}
                        </div>
                    )}
                </Row>

                {this.props.percentage !== undefined && (
                    <Row className="indicator-line">
                        <Col md={12} sm={12} xs={12} className="indicators-block">
                            <div className="indicator"></div>
                            <div
                                className="indicator green"
                                style={{
                                    width: (this.props.percentage > 100 ? 100 : this.props.percentage) + "%",
                                }}></div>
                        </Col>
                    </Row>
                )}
            </Container>
        );

        if (this.props.tooltip) {
            return (
                <OverlayTrigger placement="top" overlay={this.renderTooltip(this.props.tooltip)}>
                    {container}
                </OverlayTrigger>
            );
        }

        return container;
    }
}

export default Indicator;
