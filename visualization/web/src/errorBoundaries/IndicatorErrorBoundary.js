import React from "react";
import ErrorBoundaryHoc from "./ErrorBoundaryHoc";

const IndicatorErrorBoundary = () => {
  return (
    <div className="base-container">
      <h3>Error loading indicators</h3>
    </div>
  );
};

export default ErrorBoundaryHoc(IndicatorErrorBoundary);
