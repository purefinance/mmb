import React from "react";
import { List } from "../../components/Balances";
import { BodyErrorBoundary } from "../../errorBoundaries";
import { Container } from "react-bootstrap";
import "./index.css";
import Spinner from "../../controls/Spinner";

class Balances extends React.Component {
  async componentDidMount() {
    await this.updateSubscription();
  }

  async componentDidUpdate() {
    await this.updateSubscription();
  }

  async componentWillUnmount() {
    await this.props.ws.unsubscribeBalances();
  }

  async updateSubscription() {
    await this.props.ws.subscribeBalances();
  }

  render() {
    const {
      state: { isConnected, subscribedBalances, balances },
    } = this.props.ws;
    return (
      <Container className="base-container">
        {isConnected && subscribedBalances && balances ? (
          <BodyErrorBoundary>
            <List data={balances} />
          </BodyErrorBoundary>
        ) : (
          <Spinner />
        )}
      </Container>
    );
  }
}

export default Balances;
