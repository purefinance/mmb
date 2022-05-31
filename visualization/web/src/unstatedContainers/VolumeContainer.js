import {Container} from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class VolumeContainer extends Container {
    state = {
        interval: "",
        exchangeName: "",
        currencyPair: "",
        volumeIndicators: null,
    };

    async updateVolume(currencyPair, exchangeName, interval) {
        await this.setState({
            currencyPair: currencyPair,
            exchangeName: exchangeName,
            interval: interval,
            volumeIndicators: null,
        });

        if (exchangeName && currencyPair && interval !== "now") {
            var volumeIndicators = await CryptolpAxios.getVolumeIndicators(exchangeName, currencyPair, interval);
            await this.setState({volumeIndicators: volumeIndicators});
        }
    }

    async stopLoad() {
        CryptolpAxios.stopTryingGetResponses();
    }

    async clearData() {
        await this.setState({
            interval: "",
            exchangeName: "",
            currencyPair: "",
            volumeIndicators: null,
        });
    }
}

export default VolumeContainer;
