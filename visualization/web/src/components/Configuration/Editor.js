import React, { Component } from "react";
import { Row, Col } from "react-bootstrap";
import LaddaButton from "react-ladda/dist/LaddaButton";

class Editor extends Component {
  typingTimer;
  constructor(prop) {
    super(prop);
    this.state = {
      text: "",
      checking: false,
      isError: false,
      currentLineNumber: 0,
      currentColumnIndex: 0,
    };

    this.onChangeText = this.onChangeText.bind(this);
    this.saveConfig = this.saveConfig.bind(this);
    this.validateConfig = this.validateConfig.bind(this);
  }

  componentDidMount() {
    this.setState({ text: this.props.value });
  }

  runValidateTimer(timeoutMs) {
    clearTimeout(this.typingTimer);
    this.typingTimer = setTimeout(async () => {
      await this.validateConfig();
    }, timeoutMs);
  }

  async onChangeText(event) {
    await this.setState({ isError: false, text: event.target.value });
    this.runValidateTimer(1000);
  }

  async validateConfig() {
    await this.props.validateConfig(this.state.text);
  }

  async saveConfig() {
    await this.props.saveConfig(this.state.text);
  }

  handleCursor() {
    let textLines = this.container.value
      .substr(0, this.container.selectionStart)
      .split("\n");
    this.setState({ currentLineNumber: textLines.length });
    this.setState({
      currentColumnIndex: textLines[textLines.length - 1].length,
    });
  }

  render() {
    return (
      <Col>
        <p className={"row-index"}>
          {this.state.currentLineNumber} : {this.state.currentColumnIndex}
        </p>
        <Row>
          <textarea
            className="editor-container"
            ref={(elem) => (this.container = elem)}
            defaultValue={this.props.value}
            onChange={this.onChangeText}
            onKeyDown={this.handleCursor.bind(this)}
            onClick={this.handleCursor.bind(this)}
          />
        </Row>
        <Row>
          <LaddaButton
            disabled={this.state.isError || !this.props.isValid}
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
