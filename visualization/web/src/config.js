export default {
    baseURL: `${window.location.port === "3000" ? "http://localhost:53938" : window.location.origin}/api/`,
    // baseHubURL: `${window.location.port === "3000" ? "http://localhost:53938" : window.location.origin}/hub/`,
    baseHubURL: "ws://localhost:8080/ws",
};
