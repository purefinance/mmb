import React from "react";
import ErrorBoundaryHoc from "./ErrorBoundaryHoc";

const BodyErrorBoundary = (props) => {
    return (
        <div className="main-section">
            <div className="container-chart-and-transactions base-container">
                {props.errorMessage || "Error happened while loading data"}
            </div>
        </div>
    );
};

export default ErrorBoundaryHoc(BodyErrorBoundary);
