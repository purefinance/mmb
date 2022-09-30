# The crate with implementation of exchange client for Interactive Brokers.

## Notes

### Symbol list

Before you run the exchange client, you must make sure that you've put `symbols.csv` file to the directory of execution.

#### Required CSV-file format:
```csv
Symbol,Date,Open,High,Low,Close,Volume
A,09-Sep-2022,135.98,137.92,135.43,137.63,2425200
AA,09-Sep-2022,50.4,53.07,50.27,52.62,7269500
AAC,09-Sep-2022,9.91,9.92,9.91,9.91,53300
```

Link to download CSV-file with symbols: http://www.eoddata.com/symbols.aspx

#### Links with questions/answers about symbol list:
- https://stackoverflow.com/questions/29876693/interactive-brokers-symbol-list
- https://www.reddit.com/r/algotrading/comments/rboh48/interactive_brokers_api_getting_a_list_of_stock/
