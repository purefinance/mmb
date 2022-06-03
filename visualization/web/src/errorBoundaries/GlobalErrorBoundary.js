import React from "react";
import ErrorBoundaryHoc from "./ErrorBoundaryHoc";

const GlobalErrorBoundary = () => {
  return (
    <div className="base-container">
      <h3>Error loading page</h3>
    </div>
  );
};

export default ErrorBoundaryHoc(GlobalErrorBoundary);
