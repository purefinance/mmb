# Bitmex common information

Documentation is [here](https://www.bitmex.com/app/apiOverview)

There is also an [API description](https://bitmex.freshdesk.com/en/support/solutions/folders/13000015613)

And more [knowledge base](https://bitmex.freshdesk.com/en/support/solutions)

# Bitmex implementation features

In current implementation we do not request indecies' symbols. To do this you should change GET request from **/api/v1/instrument/active** to **/instrument/activeAndIndices**.

We get only top 25 of order book, that's enough for now. To get full order book you should subscribe to **orderBookL2** (now it's **orderBookL2_25**) via websocket.

We work only with **Perpetual Contracts** for now in derivative mode and with **Spot** in non-derivative mode.

When we get wallet balance each currency quantity must be multiplied by a rate.
We receive rates for all wallet currencies just after symbols receiving
