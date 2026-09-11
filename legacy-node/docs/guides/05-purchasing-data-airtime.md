# Purchasing data & airtime (Nigeria)

## `POST /purchase/data`
```json
{ "planId": 0, "phoneNumber": "string", "network": "MTN" }
```
→
```json
{ "success": true, "data": { "reference": "...", "plan_name": "...", "amount": 0, "phone_number": "...", "message": "..." } }
```

## `POST /purchase/airtime`
```json
{ "network": "MTN", "phoneNumber": "string", "amount": 0 }
```
→
```json
{ "success": true, "data": { "reference": "...", "amount": 0, "phone_number": "...", "message": "..." } }
```

`reference` from either response is what you'd look up later via
`GET /transactions` to confirm final status (both purchase responses
return `200` immediately — confirm whether that means the purchase is
already final or still `pending`; the transaction-status enum in the
spec is marked `CONFIRM` for the same reason).

For a second country's data/airtime endpoints, see
`08-adding-a-country-rail.md` rather than adding a `country` field
here.
