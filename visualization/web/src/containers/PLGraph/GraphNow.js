import React from "react";
import { Line } from "react-chartjs-2";

const GraphNow = (props) => {
  const { data } = props;

  const dataset = () => {
    const graphData = [];

    const labels = [];
    if (data) {
      data.forEach((t) => {
        labels.push(new Date(t.dateTime));
        graphData.push({
          t: new Date(t.dateTime),
          y: t.usdProfitChange,
        });
      });
    }

    return {
      labels: labels,
      datasets: [
        {
          data: graphData,
        },
      ],
    };
  };

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
            unit: "hour",
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
    },
  };

  const graphData = dataset();
  return <Line height={250} data={graphData} options={options} />;
};

export default GraphNow;
