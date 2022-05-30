import React, {Component} from "react";
import {NavLink} from "react-router-dom";
import {Col, Row} from "react-bootstrap";
import "./Timewrapper.css";

class Timewrapper extends Component {
    render() {
        const selectedInterval = this.props.hideNow && this.props.interval === "now" ? "day" : this.props.interval;
        const intervals = [
            {interval: "month", text: "Month"},
            {interval: "week", text: "Week"},
            {interval: "day", text: "Day"},
        ];

        const items = intervals.map((i) => {
            const isActive = i.interval === selectedInterval;
            return (
                <NavLink
                    key={i.interval}
                    className={`time-block ${isActive ? "active" : ""}`}
                    to={`${this.props.path}/${i.interval}/${this.props.exchangeName}/${this.props.currencyPair}`}>
                    <div className={`date-link ${isActive ? "active" : "noneactive"}`}>{i.text}</div>
                </NavLink>
            );
        });

        return (
            <Col md={{span: 5, offset: 1}}>
                <Row className="time-wrapper">
                    {items}
                    {!this.props.hideNow && (
                        <NavLink
                            exact
                            to={`${this.props.path}/now/${this.props.exchangeName}/${this.props.currencyPair}`}
                            className={`time-block ${this.props.interval === "now" ? "active" : ""}`}>
                            <div className={`date-link ${this.props.interval === "now" ? "active" : "noneactive"}`}>
                                Now
                            </div>
                        </NavLink>
                    )}
                </Row>
            </Col>
        );
    }
}

export default Timewrapper;
