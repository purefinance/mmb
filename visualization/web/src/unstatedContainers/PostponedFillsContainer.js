import {Container} from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class PostponedFillsContainer extends Container {
    constructor(props) {
        super(props);
        this.state = {
            loading: false,
            postponedFills: null,
            modalData: null,
        };

        this.load = this.load.bind(this);
        this.stopLoad = this.stopLoad.bind(this);
        this.showModal = this.showModal.bind(this);
    }

    async load() {
        await this.setState({loading: true});
        const postponedFills = await CryptolpAxios.getPostponedFills();
        await this.setState({loading: false, postponedFills: postponedFills});
    }

    async stopLoad() {
        CryptolpAxios.stopTryingGetResponses();
    }

    async showModal(data) {
        if (!data && !this.state.modalData) {
            return;
        }

        const newState = Object.assign({}, this.state);
        newState.modalData = data;
        await this.setState(newState);
    }
}

export default PostponedFillsContainer;
