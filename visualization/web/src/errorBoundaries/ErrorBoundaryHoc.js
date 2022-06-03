import React from "react";

function ErrorBoundaryHoc(WrappedComponent) {
  // eslint-disable-next-line react/display-name
  return class extends React.Component {
    constructor(props) {
      super(props);
      this.state = { hasError: false };
    }

    // eslint-disable-next-line no-unused-vars
    static getDerivedStateFromError(error) {
      return { hasError: true };
    }

    componentDidCatch(error, info) {
      console.log(error, info);
    }

    render() {
      if (this.state.hasError) {
        return <WrappedComponent {...this.props} />;
      }

      return this.props.children;
    }
  };
}

export default ErrorBoundaryHoc;
