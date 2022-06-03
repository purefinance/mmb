import React from "react";
import ErrorBoundaryHoc from "./ErrorBoundaryHoc";

const NavigationErrorBoundary = () => {
  return (
    <nav className="nav-menu w-clearfix w-nav-menu">
      error when loading navigation
    </nav>
  );
};

export default ErrorBoundaryHoc(NavigationErrorBoundary);
