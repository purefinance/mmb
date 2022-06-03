import React from "react";
import Spinner from "../../controls/Spinner";
import { BodyErrorBoundary } from "../../errorBoundaries";
import GraphNow from "../../components/Volume/GraphNow";
import Trades from "../../components/Trades/Trades";
import { Row, Col } from "react-bootstrap";

class Body extends React.Component {
  render() {
    const {
      state: { volumeNow },
    } = this.props.ws;

    return volumeNow ? (
      <React.Fragment>
        <BodyErrorBoundary>
          <Row className="base-row">
            <Col md={6} sm={6} xs={12} className="base-background">
              <GraphNow volume={volumeNow.volumes} />
            </Col>
            <Col md={6} sm={6} xs={12}>
              <Trades transactions={volumeNow.transactions} />
            </Col>
          </Row>
        </BodyErrorBoundary>
      </React.Fragment>
    ) : (
      <Spinner />
    );
  }
}

export default Body;
