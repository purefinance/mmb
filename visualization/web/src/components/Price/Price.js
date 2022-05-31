import React, {Component} from "react";
import Indicators from "./Indicators";
import {Line} from "react-chartjs-2";
import Spinner from "../../controls/Spinner";

class Price extends Component {
    data = (canvas) => {
        const ctx = canvas.getContext("2d");
        const redGradient = ctx.createLinearGradient(0, 0, 0, 800);
        redGradient.addColorStop(0, "#e88282");
        redGradient.addColorStop(1, "#e88282");

        const greenGradient = ctx.createLinearGradient(0, 0, 0, 800);
        greenGradient.addColorStop(0, "#83cc8c");
        greenGradient.addColorStop(1, "#83cc8c");

        let buyingOrderBooks = [];
        let sellingOrderBooks = [];
        if (this.props.preprocessedOrderBook.ranges) {
            this.props.preprocessedOrderBook.ranges.forEach((range) => {
                if (range.bids) {
                    range.bids.forEach((bid) => {
                        buyingOrderBooks.push({
                            x: new Date(bid.dateTime),
                            y: bid.value,
                        });
                    });
                }

                if (range.asks) {
                    range.asks.forEach((ask) => {
                        sellingOrderBooks.push({
                            x: new Date(ask.dateTime),
                            y: ask.value,
                        });
                    });
                }
            });
        }

        return {
            datasets: [
                {
                    label: "Buying OrdersBooks",
                    fill: false,
                    data: buyingOrderBooks,
                    borderColor: greenGradient,
                    pointRadius: 0,
                    pointHoverRadius: 0,
                    borderWidth: 0,
                },
                {
                    label: "Selling OrdersBooks",
                    fill: false,
                    data: sellingOrderBooks,
                    borderColor: redGradient,
                    pointRadius: 0,
                    pointHoverRadius: 0,
                    borderWidth: 0,
                },
            ],
        };
    };

    render() {
        let min;
        let max;
        if (this.props.preprocessedOrderBook.ranges) {
            this.props.preprocessedOrderBook.ranges.forEach((range) => {
                if (range.bids) {
                    range.bids.forEach((bid) => {
                        if (!min && !max) {
                            min = bid.value;
                            max = bid.value;
                        }

                        if (bid.value !== 0) {
                            min = Math.min(bid.value, min);
                            max = Math.max(bid.value, max);
                        }
                    });
                }

                if (range.asks) {
                    range.asks.forEach((ask) => {
                        if (!min && !max) {
                            min = ask.value;
                            max = ask.value;
                        }

                        if (ask.value !== 0) {
                            min = Math.min(ask.value, min);
                            max = Math.max(ask.value, max);
                        }
                    });
                }
            });
        }

        const options = {
            legend: {
                display: false,
            },
            lineTension: 0,
            animation: false,
            scales: {
                xAxes: [
                    {
                        type: "time",
                        time: {
                            unit: this.props.interval === 1 ? "hour" : "day",
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

        return (
            <div className="main-section">
                <Indicators loading={this.state.loadingCount} priceIndicators={this.props.priceIndicators} />
                {this.state.loadingCount ? (
                    <Spinner />
                ) : (
                    <div className="container-chart-and-transactions base-container">
                        <Line data={this.data} options={options} />
                    </div>
                )}
            </div>
        );
    }
}

export default Price;
