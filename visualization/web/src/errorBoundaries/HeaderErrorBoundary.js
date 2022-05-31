import React from "react";
import ErrorBoundaryHoc from "./ErrorBoundaryHoc";

const HeaderErrorBoundary = () => {
    return <div className="container base-container">error when loading header</div>;
};

export default ErrorBoundaryHoc(HeaderErrorBoundary);
