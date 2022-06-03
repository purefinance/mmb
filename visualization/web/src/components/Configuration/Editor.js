import React, { Component } from "react";
import JSONEditor from "jsoneditor";
import "jsoneditor/dist/jsoneditor.css";
import { Row, Col } from "react-bootstrap";
import LaddaButton from "react-ladda/dist/LaddaButton";

class Editor extends Component {
  constructor(prop) {
    super(prop);
    this.state = {
      json: "",
      checking: false,
      isError: false,
    };

    this.onChangeText = this.onChangeText.bind(this);
    this.saveConfig = this.saveConfig.bind(this);
  }

  componentDidMount() {
    const options = {
      modes: ["code", "form"],
      mode: "code",
      onChangeText: this.onChangeText,
    };

    this.jsoneditor = new JSONEditor(this.container, options, this.props.value);

    this.setState({ json: JSON.stringify(this.props.value) });
  }

  async onChangeText(newJson) {
    try {
      JSON.parse(newJson);
      await this.setState({ isError: false, json: newJson });
    } catch (error) {
      await this.setState({ isError: true });
    }
  }

  async saveConfig() {
    await this.props.saveConfig(this.state.json);
  }

  render() {
    return (
      <Col>
        <Row>
          <div
            className="jsoneditor-container"
            ref={(elem) => (this.container = elem)}
          />
        </Row>
        <Row>
          <LaddaButton
            disabled={this.state.isError}
            loading={this.props.savingConfig}
            onClick={() => this.saveConfig()}
          >
            Save
          </LaddaButton>
        </Row>
      </Col>
    );
  }
}

export default Editor;
