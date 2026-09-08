# Schema — running summary

Source of truth is `db/migrations/` (sequentially numbered, append-only
— see Task 56/b in `handover.md` for the full convention). This file
is a short, current index so a session doesn't have to reconstruct the
schema by reading every migration in order. **Update this file in the
same session as any migration that changes it.**

No migration has been applied to any live Supabase project yet —
this describes what the migration files in this directory *would*
create, not confirmed live state. See the "DB-Ops Handoff Process"
section in `handover.md` for how a live check (`psql` + `\dt`) is
actually run, and by whom.

## Shared conventions (locked in by migration `0001`, followed by every migration after)

- **id**: `uuid`, `default gen_random_uuid()` (via the `pgcrypto` extension)
- **timestamps**: `created_at` / `updated_at`, both `timestamptz`, `default now()` — `updated_at` kept current via the shared `set_updated_at()` trigger function, not per-table logic
- **status columns**: `text` + a `CHECK` constraint listing the allowed values, not a Postgres `enum` — easier for a later migration to extend the allowed list than an enum's `ALTER TYPE ... ADD VALUE`
- **no placeholder foreign keys**: a table doesn't get an FK to a table that doesn't exist yet just because a future task is expected to add one

## Tables

### `transactions` (migration `0001`)

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `reference` | `text` | unique (see index below) — what `GET /payout/verify` looks up by (Task 56/d-4) |
| `type` | `text` | `'payment'` \| `'payout'` — both `POST /pay` and `POST /payout` write here (Task 56/d-3) |
| `provider` | `text` | |
| `currency` | `text` | |
| `amount` | `numeric` | |
| `status` | `text` | `'pending'` \| `'success'` \| `'failed'`, default `'pending'` |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` trigger |

**Indexes:** `transactions_reference_key` — unique index on `reference`.

**No foreign keys yet.** Per Task 56/b's "no placeholder FKs" rule —
no `business_id` column just because Task 46 mentions a future
`businesses` table. Added in a later migration once that table
actually exists.

## Functions

- **`set_updated_at()`** — trigger function (migration `0001`). Keeps
  a row's `updated_at` current on any `UPDATE`. Shared by every table
  that wants this behavior going forward, not just `transactions` —
  a later migration attaches the same trigger function to a new table
  rather than redefining the logic.

## Not yet in this schema

Everything else Task 56/d's remaining sub-tasks cover: the Supabase
client wiring (d-2), the write path at `/pay`/`/payout` (d-3), the
read path at `/payout/verify` (d-4), and RLS policy design (d-5) —
none of that is schema, so none of it belongs in this file until it
produces its own migration or a schema-relevant change.
