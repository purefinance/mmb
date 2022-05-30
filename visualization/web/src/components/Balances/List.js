import React from "react";
import Currency from "./Currency";
import {Row} from "react-bootstrap";

const List = (props) => {
    const sortedData = sortByDescending(props.data);
    return (
        <Row className="base-row">
            {Object.keys(sortedData).map((currencyCode) => (
                <Currency key={currencyCode} data={sortedData[currencyCode]} currencyCode={currencyCode} />
            ))}
        </Row>
    );
};

function sortByDescending(object) {
    const result = {};
    if (object) {
        Object.keys(object)
            .sort((a, b) => object[b].exchanges.length - object[a].exchanges.length)
            .forEach((key) => {
                result[key] = object[key];
            });
    }
    return result;
}

export default List;
