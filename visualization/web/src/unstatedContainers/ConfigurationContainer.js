import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";
import { toast } from "react-toastify";

class ConfigurationContainer extends Container {
  constructor(props) {
    super(props);
    this.state = {
      loading: false,
      config: null,
      savingConfig: false,
      isValid: true,
      errorMessage: "",
    };

    this.loadConfig = this.loadConfig.bind(this);
    this.stopLoad = this.stopLoad.bind(this);
    this.saveConfig = this.saveConfig.bind(this);
    this.validateConfig = this.validateConfig.bind(this);
  }

  async loadConfig() {
    await this.setState({ loading: true });
    const config = await CryptolpAxios.getConfig();
    config.config = config.config || "";
    await this.setState({ loading: false, config: config });
    await this.validateConfig(config.config);
  }

  async validateConfig(config) {
    await CryptolpAxios.validateConfig(config).then((response) => {
      this.setState({
        isValid: response.data.valid,
        errorMessage: response.data.error || "",
      });
    });
  }

  async stopLoad() {
    CryptolpAxios.stopTryingGetResponses();
  }

  async saveConfig(text) {
    await this.setState({ savingConfig: true });
    await CryptolpAxios.saveConfig({ config: text })
      .then((response) => {
        console.log(response);
        toast.dismiss();
        toast.info("Saved!");
      })
      .catch(() => {
        toast.dismiss();
        toast.error("Something wrong!");
      });
    await this.setState({ savingConfig: false });
  }
}

export default ConfigurationContainer;
