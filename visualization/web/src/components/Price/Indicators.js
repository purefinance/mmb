import React, {Component} from "react";
import Indicator from "../Indicator/Indicator";

class Indicators extends Component {
    render() {
        return (
            <div className="boxes-block base-container">
                <Indicator
                    first
                    data={this.props.loading ? null : this.props.priceIndicators.priceChangeOverMarket}
                    isColored={true}
                    postfix="%"
                    title="Price Change Over Market"
                />
            </div>
        );
    }
}

export default Indicators;
