# Getting started

**Base URL (Nigeria, live):** `https://telco.opik.net/api/v1`

**Flow for a new business integrating this API:**

1. `POST /auth/register` — create an account, receive an `api_key`.
2. Authenticate subsequent requests with that key (see
   `02-authentication.md` — the exact header is unconfirmed, flagged
   there).
3. `GET /plans` — see what's purchasable (filter by `network`/`category`).
4. `POST /wallet/deposit` — get a virtual account number, fund it.
5. `GET /wallet` — confirm the balance landed.
6. `POST /purchase/data` or `POST /purchase/airtime` — buy for a
   customer's phone number.
7. `GET /transactions` — reconcile what happened.
8. `POST /webhooks` — register a URL so purchase/deposit events push to
   you instead of you polling `/transactions`.

All responses observed so far share one envelope:
```json
{ "success": true, "data": { ... } }
```
The error envelope was **not** captured from the live server (see
`09-conventions-and-open-items.md`) — treat the shape in the spec as a
placeholder until confirmed.

For the full field-level contract, treat `docs/openapi/openapi.yaml`
(and the files it references) as the source of truth — this guide and
its siblings are the narrative walkthrough of that same contract.
