import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class AppContainer extends Container {
  constructor(props) {
    super(props);
    CryptolpAxios.loadToken();
    this.getClientDomain();
    this.state = {
      isAuthorized: CryptolpAxios.isAuthorized,
    };
  }

  async getClientDomain() {
    const clientDomain = await CryptolpAxios.getClientDomain();
    this.setState({ clientDomain: clientDomain.content });
  }
}

export default AppContainer;
