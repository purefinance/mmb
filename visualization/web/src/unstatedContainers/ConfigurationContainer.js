import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class ConfigurationContainer extends Container {
  constructor(props) {
    super(props);
    this.state = {
      loading: false,
      config: null,
      savingConfig: false,
    };

    this.loadConfig = this.loadConfig.bind(this);
    this.stopLoad = this.stopLoad.bind(this);
    this.saveConfig = this.saveConfig.bind(this);
  }

  async loadConfig() {
    await this.setState({ loading: true });
    const config = await CryptolpAxios.getConfig();
    await this.setState({ loading: false, config: config });
  }

  async stopLoad() {
    CryptolpAxios.stopTryingGetResponses();
  }

  async saveConfig(configJSON) {
    await this.setState({ savingConfig: true });
    await CryptolpAxios.saveConfig({ config: configJSON });
    await this.setState({ savingConfig: false });
  }
}

export default ConfigurationContainer;
