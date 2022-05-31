import React from "react";
import {Bar} from "react-chartjs-2";

const Graph = (props) => {
    const {volumeIndicators, interval} = props;

    const data = () => {
        const volumeGraphData = [];

        if (volumeIndicators && volumeIndicators.volumeGraphData && volumeIndicators.volumeGraphData.ranges) {
            volumeIndicators.volumeGraphData.ranges.forEach((range) => {
                volumeGraphData.push({
                    x: new Date(range.startDateTime),
                    y: range.volume,
                });
            });
        }

        return {
            datasets: [
                {
                    label: "Volume",
                    fill: true,
                    data: volumeGraphData,
                    backgroundColor: "#727f8d",
                    borderWidth: 1,
                },
            ],
        };
    };

    let min;
    let max;
    if (volumeIndicators.volumeGraphData && volumeIndicators.volumeGraphData.ranges) {
        volumeIndicators.volumeGraphData.ranges.forEach((range) => {
            if (!min && !max) {
                min = range.volume;
                max = range.volume;
            }
            if (range.volume !== 0) {
                min = Math.min(range.volume, min);
                max = Math.max(range.volume, max);
            }
        });
    }
    const options = {
        legend: {
            display: false,
        },
        animation: false,
        scales: {
            xAxes: [
                {
                    type: "time",
                    time: {
                        unit: interval === 1 ? "hour" : "day",
                        displayFormats: {
                            hour: "hA",
                            day: "MMM D",
                        },
                    },
                    distribution: "linear",
                    ticks: {
                        beginAtZero: true,
                        autoSkip: false,
                    },
                },
            ],
            yAxes: [
                {
                    display: true,
                    ticks: {
                        suggestedMin: min,
                        suggestedMax: max,
                    },
                },
            ],
        },
    };
    let graphData = data();
    return <Bar data={graphData} options={options} />;
};

export default Graph;
