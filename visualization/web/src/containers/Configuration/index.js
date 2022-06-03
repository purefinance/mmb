import React from "react";
import { Col } from "react-bootstrap";
import "./index.css";
import Spinner from "../../controls/Spinner";
import Editor from "../../components/Configuration/Editor";

class Configuration extends React.Component {
  async componentDidMount() {
    await this.props.configuration.loadConfig();
  }

  async componentWillUnmount() {
    this.props.configuration.stopLoad();
  }

  render() {
    const {
      state: { loading, savingConfig, config },
    } = this.props.configuration;
    return !loading && config ? (
      <Col className="base-container">
        <Editor
          value={JSON.parse(config.rawConfig)}
          savingConfig={savingConfig}
          saveConfig={this.props.configuration.saveConfig}
        />
      </Col>
    ) : (
      <Spinner />
    );
  }
}

export default Configuration;
