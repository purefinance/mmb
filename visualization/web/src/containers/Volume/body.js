import React from "react";
import Spinner from "../../controls/Spinner";
import {Graph} from "../../components/Volume";
import {BodyErrorBoundary} from "../../errorBoundaries";
import {Container} from "react-bootstrap";

class Body extends React.Component {
    render() {
        const {
            state: {volumeIndicators, interval},
        } = this.props.volume;
        return volumeIndicators ? (
            <BodyErrorBoundary>
                <Container className="base-background base-container">
                    <Graph volumeIndicators={volumeIndicators} interval={interval} />
                </Container>
            </BodyErrorBoundary>
        ) : (
            <Spinner />
        );
    }
}

export default Body;
