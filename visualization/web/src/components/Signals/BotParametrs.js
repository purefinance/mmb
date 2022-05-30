import React from "react";
import Row from "./Row.js";
import "./BotParametrs.css";

function BotParametrs(props) {
    const {signalsEmulationResults} = props.signals;

    return (
        <React.Fragment>
            <div className="baseGrid base-background bot_parametrs">
                <div className="title_block_general">
                    <div className="title_regulare_instant">Profit</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">Original Partial TS</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">Original Partial TS</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">Reduced Partial TS</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">Final TS Threshold</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">Final TS Rate</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">SL Threshold Rate</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">TP Threshold</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">Proportion TS</div>
                </div>
                <div className="title_block_general">
                    <div className="title_regulare_instant">No Trade Zone Threshold</div>
                </div>
                <div className="gridLine bot"></div>
                {signalsEmulationResults &&
                    signalsEmulationResults.map((signal, id) => (
                        <Row key={id} profit={signal.profit} parameters={signal.parameters} />
                    ))}
            </div>
        </React.Fragment>
    );
}

export default BotParametrs;
