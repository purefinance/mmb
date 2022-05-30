import React from "react";
import {Col, Table, Alert, Button, Modal} from "react-bootstrap";
import Spinner from "../../controls/Spinner";
import utils from "../../utils";
import JSONEditor from "jsoneditor";

class PostponedFills extends React.Component {
    async componentDidMount() {
        await this.props.postponedFills.load();
    }

    async componentWillUnmount() {
        this.props.postponedFills.stopLoad();
    }

    renderTable(postponedFills) {
        if (!postponedFills.length) {
            return <Alert variant="warning">No data</Alert>;
        }

        let i = 1;
        return (
            <>
                <Table striped bordered hover>
                    <thead>
                        <tr>
                            <th>#</th>
                            <th>Id</th>
                            <th>Date</th>
                            <th>Hedge</th>
                            <th>Side</th>
                            <th>Price</th>
                            <th>Hedge/Target amount</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {postponedFills.map((x) => {
                            return (
                                <tr key={x.id}>
                                    <td>{i++}</td>
                                    <td title={x.id}>{x.id.slice(-12)}</td>
                                    <td>{utils.toLocalDateTime(x.fillDateTime)}</td>
                                    <td>
                                        {x.hedgeExchangeId}:{x.currencyCodePair}
                                    </td>
                                    <td>{x.hedgeDispositionSide === 1 ? "Buy" : "Sell"}</td>
                                    <td>{x.fillPrice}</td>
                                    <td>
                                        {x.postponedHedgeAmount} / {x.postponedTargetAmount}
                                    </td>
                                    <td>
                                        <Button onClick={() => this.props.postponedFills.showModal(x)}>JSON</Button>
                                    </td>
                                </tr>
                            );
                        })}
                    </tbody>
                </Table>
            </>
        );
    }

    render() {
        const {
            state: {loading, postponedFills, modalData},
        } = this.props.postponedFills;
        return !loading && postponedFills ? (
            <Col className="base-container">
                <h2>Postponed Fills</h2>
                {this.renderTable(postponedFills, modalData)}

                <Modal show={modalData ? true : false} onHide={() => this.props.postponedFills.showModal(null)}>
                    <Modal.Header closeButton>
                        <Modal.Title>JSON</Modal.Title>
                    </Modal.Header>
                    <Modal.Body>{this.renderJson(modalData)}</Modal.Body>
                </Modal>
            </Col>
        ) : (
            <Spinner />
        );
    }

    renderJson(json) {
        if (!json) return;
        return (
            <div
                className="jsoneditor-container"
                ref={(elem) => {
                    if (!elem) return;
                    this.jsoneditor = new JSONEditor(elem, {mode: "code"}, JSON.parse(JSON.stringify(json)));
                }}
            />
        );
    }
}

export default PostponedFills;
