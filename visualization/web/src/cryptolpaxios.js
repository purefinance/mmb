import axios from "axios";
import decode from "jwt-decode";
import config from "./config.js";
import {delay} from "q";

export default class CryptolpAxios {
    static axiosInstance = axios.create({
        baseURL: config.baseURL,
    });

    static token = null;
    static userInfo = null;
    static expiration = null;
    static isAuthorized = false;
    static role = "";
    static clientType = "";
    static userUpdatedListners = [];
    static notStopedRequests = ["supportedExchanges", "users", "roles", "clientDomain", "clientType"];

    static allStartedRequests = {
        //...all fields will be added automaticaly
    };

    static stopTryingGetResponses() {
        for (const [key] of Object.entries(CryptolpAxios.allStartedRequests)) {
            if (!CryptolpAxios.notStopedRequests.includes(key)) CryptolpAxios.allStartedRequests[key] = false;
        }
    }

    static async getResponse(requestName, request) {
        CryptolpAxios.allStartedRequests[requestName] = true;

        while (CryptolpAxios.allStartedRequests[requestName]) {
            const response = await CryptolpAxios.axiosInstance.get(request);
            if (response.data) {
                CryptolpAxios.allStartedRequests[requestName] = false;
                return response.data;
            } else {
                console.log(`Can't fetch ${request}`);
                await delay(5000);
            }
        }
    }

    static saveConfig(config) {
        return CryptolpAxios.axiosInstance.put(`Configuration`, config);
    }

    static getTrades(strategyNames, exchangeName, currencyCodePair, skip, count) {
        return CryptolpAxios.axiosInstance.post(`Liquidity/Trades`, {
            strategyNames,
            exchangeName,
            currencyCodePair,
            skip,
            count,
        });
    }

    static getConfig() {
        return this.getResponse("config", `Configuration`);
    }

    static getPostponedFills() {
        return this.getResponse("postponedFills", "Liquidity/PostponedFills");
    }

    static getSignals(exchangeName, currencyPair) {
        return this.getResponse("signals", `Signals?exchangeName=${exchangeName}&currencyPair=${currencyPair}`);
    }

    static getExplanations(exchangeName, currencyCodePair) {
        return this.getResponse(
            "explanations",
            `Explanation?exchangeName=${exchangeName}&currencyCodePair=${currencyCodePair}`,
        );
    }

    static getPLGraph(exchangeName, currencyCodePair) {
        return this.getResponse(
            "plGraph",
            `ProfitLoss?exchangeName=${exchangeName}&currencyCodePair=${currencyCodePair}`,
        );
    }

    static getSupportedExchanges() {
        return this.getResponse("supportedExchanges", `Liquidity/SupportedExchanges`);
    }

    static getBalances() {
        return this.getResponse("balance", `rebalancing`);
    }

    static getAllBalances() {
        return this.getResponse("allBalances", `balance`);
    }

    static getVolumeIndicators(exchangeName, currencyPair, preProccessingInterval) {
        return this.getResponse(
            "volumeIndicators",
            `volumes?exchangeName=${exchangeName}&currencyPair=${currencyPair}&preProccessingInterval=${preProccessingInterval}`,
        );
    }

    static getLiquidityIndicators(exchangeName, currencyPair, preProccessingInterval) {
        return this.getResponse(
            "liquidityIndicators",
            `Liquidity?exchangeName=${exchangeName}&currencyPair=${currencyPair}&preProccessingInterval=${preProccessingInterval}`,
        );
    }

    static getUsers(email) {
        return this.getResponse("users", `users?email=${email}`);
    }

    static getRoles() {
        return this.getResponse("roles", `users/roles`);
    }

    static getClientType() {
        return this.getResponse("clientType", `account/clienttype`);
    }

    static updateUser(user) {
        return CryptolpAxios.axiosInstance.put(`users`, user);
    }

    static getClientDomain() {
        return this.getResponse("clientDomain", "account/clientdomain");
    }

    static login(user) {
        return CryptolpAxios.axiosInstance.post(`account/login`, user);
    }

    static register(user) {
        return CryptolpAxios.axiosInstance.post(`account/register`, user);
    }

    static setToken(data, clienttype) {
        localStorage.setItem("auth_token", data.token);
        localStorage.setItem("auth_expiration", data.expiration);
        localStorage.setItem("auth_role", data.role);
        localStorage.setItem("auth_role", data.role);
        localStorage.setItem("client_type", clienttype);
        CryptolpAxios.token = data.token;
        CryptolpAxios.role = data.role;
        CryptolpAxios.expiration = data.expiration;
        CryptolpAxios.clientType = clienttype;
        CryptolpAxios.loadUser();
    }

    static logout = () => {
        CryptolpAxios.token = null;
        CryptolpAxios.userInfo = null;
        CryptolpAxios.expiration = null;
        CryptolpAxios.isAuthorized = false;
        localStorage.removeItem("auth_token");
        localStorage.removeItem("auth_expiration");
        localStorage.removeItem("auth_role");
        CryptolpAxios.userUpdated();
    };

    static userUpdated = () => {
        CryptolpAxios.userUpdatedListners.forEach((listner) => {
            listner();
        });
    };

    static loadUser = () => {
        CryptolpAxios.isAuthorized = true;
        CryptolpAxios.axiosInstance.defaults.headers.common["Authorization"] = "Bearer " + CryptolpAxios.token;
        CryptolpAxios.userInfo = decode(CryptolpAxios.token);
        CryptolpAxios.userUpdated();
    };

    static loadToken = () => {
        CryptolpAxios.isAuthorized = true;
        // Disable auth

        // CryptolpAxios.axiosInstance.interceptors.request.use((request) => {
        //     return request;
        // });
        // CryptolpAxios.axiosInstance.interceptors.response.use(
        //     (response) => {
        //         return response;
        //     },
        //     (error) => {
        //         if (
        //             error.response &&
        //             (error.response.status === 401 || error.response.status === 403) &&
        //             CryptolpAxios.isAuthorized
        //         ) {
        //             localStorage.removeItem("auth_token");
        //             localStorage.removeItem("auth_expiration");
        //             localStorage.removeItem("auth_role");
        //             window.location.href = "/login";
        //         }
        //         return error;
        //     },
        // );
        // if (!CryptolpAxios.token) {
        //     CryptolpAxios.token = localStorage.getItem("auth_token");
        //     CryptolpAxios.expiration = localStorage.getItem("auth_expiration");
        //     CryptolpAxios.role = localStorage.getItem("auth_role");
        //     CryptolpAxios.clientType = localStorage.getItem("client_type");
        // }
        // if (CryptolpAxios.token) CryptolpAxios.loadUser();
    };
}
