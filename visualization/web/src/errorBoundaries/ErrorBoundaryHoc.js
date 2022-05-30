import React from "react";

function ErrorBoundaryHoc(WrappedComponent) {
    return class extends React.Component {
        constructor(props) {
            super(props);
            this.state = {hasError: false};
        }

        static getDerivedStateFromError(error) {
            return {hasError: true};
        }

        componentDidCatch(error, info) {
            //logerror here
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
