import React from "react";
import Exchange from "../../components/Dashboard/Exchange";
import Spinner from "../../controls/Spinner";
import { Col, Row } from "react-bootstrap";
import "./Dashboard.css";

class Dashboard extends React.Component {
  async componentDidMount() {
    await this.props.ws.subscribeDashboard();
  }

  async componentDidUpdate() {
    await this.props.ws.subscribeDashboard();
  }

  async componentWillUnmount() {
    await this.props.ws.unsubscribeDashboard();
  }

  render() {
    const {
      state: { dashboardIndicators, subscribedDashboard },
    } = this.props.ws;

    const exchanges = [];

    if (dashboardIndicators) {
      dashboardIndicators.forEach((indicator, index) => {
        exchanges.push(
          <Exchange key={index} dashboardIndicators={indicator} />
        );
      });
    }

    return (
      <Col className="base-container">
        {this.props.ws.state.isConnected &&
        subscribedDashboard &&
        dashboardIndicators ? (
          <React.Fragment>
            <Col className="exchangesCol">
              <Row className="exchangesRow">{exchanges}</Row>
            </Col>
          </React.Fragment>
        ) : (
          <Spinner />
        )}
      </Col>
    );
  }
}

export default Dashboard;
