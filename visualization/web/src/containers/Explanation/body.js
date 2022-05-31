import React from "react";
import {Container, Row, Col} from "react-bootstrap";
import {DateTime} from "luxon";
import "./body.css";

class Body extends React.Component {
    constructor(prop) {
        super(prop);
        this.state = {
            selectIndex: 0,
            selectedIndexes: {},
        };

        this.onSelect = this.onSelect.bind(this);
        this.onSelectExplanation = this.onSelectExplanation.bind(this);
    }

    onSelect(index) {
        this.setState({selectIndex: index});
    }

    onSelectExplanation(index) {
        const selectedIndexes = this.state.selectedIndexes;

        if (selectedIndexes[index]) {
            selectedIndexes[index] = !selectedIndexes[index];
        } else {
            selectedIndexes[index] = true;
        }

        this.setState({selectedIndexes});
    }

    render() {
        const {explanations} = this.props;

        const elements = [];
        const forElements = {};

        if (explanations) {
            explanations.forEach((element, index) => {
                elements.push(
                    <Row
                        key={index}
                        className={`base-row explanation ${this.state.selectIndex === index ? "selected" : ""}`}
                        onClick={(event) => this.onSelect(index)}>
                        {element.id}: {DateTime.fromISO(element.dateTime).toFormat("HH:mm:ss.SSS")}
                    </Row>,
                );
                forElements[index] = [];
                element.priceLevels.forEach((el, i) => {
                    forElements[index].push(
                        <Row
                            key={i}
                            className={`base-row explanation-row ${this.state.selectedIndexes[i] ? "high" : ""}`}>
                            <Col md={1} className="base-col center arrowCol">
                                <i
                                    className={`fas fa-angle-down icon-arrow cursor`}
                                    onClick={(e) => this.onSelectExplanation(i)}></i>
                            </Col>
                            <Col md={2} className="base-col">
                                {el.modeName}
                            </Col>
                            <Col md={1} className="base-col center">
                                {el.price}
                            </Col>
                            <Col md={1} className="base-col center">
                                {el.amount}
                            </Col>
                            <Col md={7} className="base-col">
                                {el.reasons.map((e, elIndex) => (
                                    <Row key={elIndex} className="base-row bottom-border">
                                        {e}
                                    </Row>
                                ))}
                            </Col>
                        </Row>,
                    );
                });
            });
        }

        return (
            <Container className="base-background base-container">
                <Row className="base-row">
                    <Col md={3} className="base-col explanation-scroll-box right-border">
                        {elements}
                    </Col>
                    <Col md={9} className="base-col">
                        <Row className={`base-row explanation-row`}>
                            <Col md={1} className="base-col center"></Col>
                            <Col md={2} className="base-col center">
                                Mode
                            </Col>
                            <Col md={1} className="base-col center">
                                Price
                            </Col>
                            <Col md={1} className="base-col center">
                                Amount
                            </Col>
                            <Col md={7} className="base-col center">
                                Reasons
                            </Col>
                        </Row>
                        {forElements && forElements[this.state.selectIndex]}
                    </Col>
                </Row>
            </Container>
        );
    }
}

export default Body;
