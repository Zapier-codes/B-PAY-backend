# Transactions

**`GET /transactions?limit=20&offset=0`**

```json
{
  "success": true,
  "data": [
    { "id": "...", "type": "deposit", "amount": 0, "status": "pending", "reference": "...", "created_at": "2026-09-06T17:43:23.312Z" }
  ]
}
```

**Unconfirmed — flagged in `components/schemas.yaml`:** only one
`type` value (`"deposit"`) and one `status` value (`"pending"`)
appeared in the captured example. The real full sets almost certainly
include something like `purchase`/`airtime` types and
`success`/`failed` statuses, but don't hardcode against a guess —
confirm the actual enum against the live server (make a real purchase
and deposit in test mode, then check what values come back) before
building any client-side logic that branches on these fields.

This is also the reconciliation surface once the Supabase-backed
dashboard (see `handover.md`) exists — the plan there is for the
dashboard to mirror this same transaction data per business rather
than replace this endpoint, so this response shape becomes the schema
`handover.md`'s proposed Supabase `transactions` table is designed to
match. Keep the two in sync as this evolves.
