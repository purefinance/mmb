import React from "react";

function Row(props) {
    return (
        <React.Fragment>
            <div className="amount_item">
                <div>{props.profit}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.originalPartialTrailingStopRate}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.reducedPartialTrailingStopRate}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.finalTrailingStopThreshold}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.finalTrailingStopLossRate}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.stopLossRate}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.takeProfitRate}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.proportionOfTrailingStop}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.noTradeZoneTreshold}</div>
            </div>
            <div className="amount_item">
                <div>{props.parameters.numberOfReentrency}</div>
            </div>
        </React.Fragment>
    );
}

export default Row;
