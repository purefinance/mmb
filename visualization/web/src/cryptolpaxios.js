import axios from "axios";
import decode from "jwt-decode";
import config from "./config.js";
import { delay } from "q";
import { toast } from "react-toastify";

export default class CryptolpAxios {
  static axiosInstance = axios.create({
    baseURL: config.baseURL,
  });

  static token = null;
  static refreshToken = null;
  static userInfo = null;
  static expiration = null;
  static isAuthorized = false;
  static role = "";
  static clientType = "";
  static userUpdatedListners = [];
  static notStopedRequests = [
    "supportedExchanges",
    "users",
    "roles",
    "clientDomain",
    "clientType",
  ];

  static allStartedRequests = {
    //...all fields will be added automaticaly
  };

  static stopTryingGetResponses() {
    for (const [key] of Object.entries(CryptolpAxios.allStartedRequests)) {
      if (!CryptolpAxios.notStopedRequests.includes(key))
        CryptolpAxios.allStartedRequests[key] = false;
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
    return CryptolpAxios.axiosInstance.put(`configuration`, config);
  }

  static validateConfig(config) {
    return CryptolpAxios.axiosInstance.post(`configuration/validate`, {
      config,
    });
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
    return this.getResponse("config", `configuration`);
  }

  static getPostponedFills() {
    return this.getResponse("postponedFills", "Liquidity/PostponedFills");
  }

  static getSignals(exchangeName, currencyPair) {
    return this.getResponse(
      "signals",
      `Signals?exchangeName=${exchangeName}&currencyPair=${currencyPair}`
    );
  }

  static getExplanations(exchangeName, currencyCodePair) {
    return this.getResponse(
      "explanations",
      `Explanation?exchangeName=${exchangeName}&currencyCodePair=${currencyCodePair}`
    );
  }

  static getPLGraph(exchangeName, currencyCodePair) {
    return this.getResponse(
      "plGraph",
      `ProfitLoss?exchangeName=${exchangeName}&currencyCodePair=${currencyCodePair}`
    );
  }

  static getSupportedExchanges() {
    return this.getResponse(
      "supportedExchanges",
      `liquidity/supported-exchanges`
    );
  }

  static getBalances() {
    return this.getResponse("balance", `rebalancing`);
  }

  static getAllBalances() {
    return this.getResponse("allBalances", `balance`);
  }

  static getVolumeIndicators(
    exchangeName,
    currencyPair,
    preProccessingInterval
  ) {
    return this.getResponse(
      "volumeIndicators",
      `volumes?exchangeName=${exchangeName}&currencyPair=${currencyPair}&preProccessingInterval=${preProccessingInterval}`
    );
  }

  static getLiquidityIndicators(
    exchangeName,
    currencyPair,
    preProccessingInterval
  ) {
    return this.getResponse(
      "liquidityIndicators",
      `Liquidity?exchangeName=${exchangeName}&currencyPair=${currencyPair}&preProccessingInterval=${preProccessingInterval}`
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

  static loginByRefreshToken(payload) {
    return CryptolpAxios.axiosInstance.post("account/refresh-token", payload);
  }

  static setToken(data, clienttype) {
    localStorage.setItem("auth_token", data.token);
    localStorage.setItem("auth_expiration", data.expiration);
    localStorage.setItem("auth_role", data.role);
    localStorage.setItem("client_type", clienttype);
    localStorage.setItem("refresh_token", data.refreshToken);
    CryptolpAxios.token = data.token;
    CryptolpAxios.role = data.role;
    CryptolpAxios.expiration = data.expiration;
    CryptolpAxios.clientType = clienttype;
    CryptolpAxios.refreshToken = data.refreshToken;
    CryptolpAxios.loadUser();
  }

  static logout = () => {
    CryptolpAxios.token = null;
    CryptolpAxios.refreshToken = null;
    CryptolpAxios.userInfo = null;
    CryptolpAxios.expiration = null;
    CryptolpAxios.isAuthorized = false;
    localStorage.removeItem("auth_token");
    localStorage.removeItem("auth_expiration");
    localStorage.removeItem("auth_role");
    localStorage.removeItem("refresh_token");
    CryptolpAxios.userUpdated();
    window.location.href = "/login";
  };

  static userUpdated = () => {
    CryptolpAxios.userUpdatedListners.forEach((listner) => {
      listner();
    });
  };

  static loadUser = () => {
    CryptolpAxios.isAuthorized = true;
    CryptolpAxios.axiosInstance.defaults.headers.common["Authorization"] =
      "Bearer " + CryptolpAxios.token;
    CryptolpAxios.userInfo = decode(CryptolpAxios.token);
    CryptolpAxios.userUpdated();
  };

  static loadToken = () => {
    CryptolpAxios.axiosInstance.interceptors.request.use((request) => {
      return request;
    });
    CryptolpAxios.axiosInstance.interceptors.response.use(
      (response) => {
        return response;
      },
      (error) => {
        const originalRequest = error.config;
        console.error(error);
        if (error.response) {
          if (
            (error.response.status === 401 || error.response.status === 403) &&
            CryptolpAxios.isAuthorized &&
            !originalRequest._retry
          ) {
            originalRequest._retry = true;
            let refreshToken = localStorage.getItem("refresh_token");
            // Trying to authorize by refresh token
            if (refreshToken) {
              this.loginByRefreshToken({ refreshToken })
                .then(async (response) => {
                  const clientType = await CryptolpAxios.getClientType();
                  CryptolpAxios.setToken(response.data, clientType);
                })
                .catch((err) => {
                  console.error(err);
                  CryptolpAxios.logout();
                });
            } else {
              CryptolpAxios.logout();
            }
          } else {
            if (error.response.status === 0) {
              toast.error("Connection problem");
              toast.clearWaitingQueue();
            }
          }
        } else if (error.request) {
          toast.error("Connection problem");
          toast.clearWaitingQueue();
        } else {
          toast.error("Something wrong");
          toast.clearWaitingQueue();
        }
        return Promise.reject(error);
      }
    );
    if (!CryptolpAxios.token) {
      CryptolpAxios.token = localStorage.getItem("auth_token");
      CryptolpAxios.expiration = localStorage.getItem("auth_expiration");
      CryptolpAxios.role = localStorage.getItem("auth_role");
      CryptolpAxios.clientType = localStorage.getItem("client_type");
      CryptolpAxios.refreshToken = localStorage.getItem("refresh_token");
    }
    if (CryptolpAxios.token) CryptolpAxios.loadUser();
  };
}
