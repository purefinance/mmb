import React from "react";
import { Col, Row } from "react-bootstrap";
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
      state: { loading, savingConfig, config, isValid, errorMessage },
    } = this.props.configuration;
    return !loading && config ? (
      <Col className="base-container">
        <Col>
          <Row>
            {isValid ? (
              <h4 className={"text-success"}>Config is valid</h4>
            ) : (
              <div>
                <h4 className={"text-danger"}>Config is invalid</h4>
                <p className={"text-danger"}>{errorMessage}</p>
              </div>
            )}
          </Row>
        </Col>
        <Editor
          value={config.config}
          savingConfig={savingConfig}
          saveConfig={this.props.configuration.saveConfig}
          validateConfig={this.props.configuration.validateConfig}
          isValid={isValid}
        />
      </Col>
    ) : (
      <Spinner />
    );
  }
}

export default Configuration;
