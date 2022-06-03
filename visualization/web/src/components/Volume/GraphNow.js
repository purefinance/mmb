import React from "react";
import { Bar } from "react-chartjs-2";

const GraphNow = (props) => {
  const { volume } = props;

  const data = () => {
    const volumeGraphData = [];

    if (volume) {
      volume.forEach((t) => {
        volumeGraphData.push({
          x: new Date(t.dateTime),
          y: t.amount,
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
  if (volume) {
    volume.forEach((t) => {
      if (!min && !max) {
        min = t.amount;
        max = t.amount;
      }
      if (t.amount !== 0) {
        min = Math.min(t.amount, min);
        max = Math.max(t.amount, max);
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

  const graphData = data();
  return <Bar height={250} data={graphData} options={options} />;
};

export default GraphNow;
