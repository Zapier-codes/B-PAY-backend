# Schema — running summary

Source of truth is `db/migrations/` (sequentially numbered, append-only
— see Task 56/b in `handover.md` for the full convention). This file
is a short, current index so a session doesn't have to reconstruct the
schema by reading every migration in order. **Update this file in the
same session as any migration that changes it.**

**Migrations `0001`, `0003`, and `0004` are confirmed live** (2026-09-08)
— `0001` applied by the product owner via `psql -f`, from the second
(proot-distro Ubuntu) environment, against project ref
`mfekzzwsoiezqkovabmp`; `0003`/`0004` applied the same way, same
session as this update (`CREATE TABLE` / `CREATE TRIGGER` / `ALTER
TABLE` / `CREATE POLICY` all confirmed — the `DROP TRIGGER IF EXISTS`
"does not exist, skipping" notice is expected on a brand-new table,
same as `0001` saw for `transactions`). Every migration in
`db/migrations/` not listed here still needs its own confirmation the
same way before this file should be treated as describing live state
for it — check `handover.md`'s per-migration notes, or run
`\dt`/`\d <table>` yourself, rather than assuming everything in this
directory has been applied just because some of it has. Migration
`0002` (transactions RLS) is not yet confirmed live as of this update.

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

**Row Level Security:** enabled (migration `0002`). One explicit
policy, `transactions_service_role_all`, scoped to `service_role`
only (this backend's own connection role — redundant with
`service_role`'s own BYPASSRLS, written explicitly anyway so intent
is documented in-migration). No policy for `anon`/`authenticated` —
under RLS, no matching policy means no access, which is the safe
default until Task 46's dashboard actually needs a real,
per-business-scoped policy here.

### `customers` (migration `0003`) — the "Customer Vault", Task 57 Piece 2

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key — this is the `customer_id` a caller passes back on a later request (Task 57/d, not yet built) |
| `first_name` | `text` | nullable |
| `last_name` | `text` | nullable |
| `phone_number` | `text` | nullable |
| `billing_address` | `text` | nullable |
| `customer_type` | `text` | nullable — named `customer_type`, not `type`, to avoid confusion with `transactions.type` (a different concept). No `CHECK` constraint yet — unlike `transactions.status`/`transactions.type`, this column's full allowed-value set hasn't been independently confirmed against provider docs, so it's left unconstrained rather than guessed |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` trigger |

**Deliberately not columns here, per Task 57's own writeup:**
- `email` — always supplied fresh via the canonical `customer.email`
  on every request; never vaulted.
- `ip_address` — request-time network context, not a durable customer
  attribute (Task 57's own "important nuance" paragraph lists exactly
  four durable fields — name, phone, billing address, customer type —
  and IP address isn't one of them). Still supplied fresh, per call,
  via `provider_data.<provider>.customer.ip_address` the same as
  today.
- `order`/`description` (or anything per-transaction) — Task 57 is
  explicit these are per-purchase, not per-customer, and must never be
  vaulted regardless of provider.

**Every column except `id`/timestamps is nullable** — this is a
general-purpose vault, not scoped to one provider's exact requirement
set, so it must hold a partially-populated profile. Task 57/d's own
resolution order (request field → vaulted row → 400 naming the
still-missing field) is what enforces "required for this provider",
not this table's constraints.

**No foreign keys yet.** Nothing in this repo currently references
`customers.id` from another table.

**Row Level Security:** enabled (migration `0004`). One explicit
policy, `customers_service_role_all`, scoped to `service_role` only —
same pattern as `transactions_service_role_all` above, same reasoning.
No policy for `anon`/`authenticated`.

**Not yet wired to anything.** Task 57/d (vault read/write logic) and
Task 57/e (`/pay` end-to-end wiring) are still open — this table is
now live in the schema (confirmed above) but nothing in the running
application reads from or writes to it as of this update.

## Functions

- **`set_updated_at()`** — trigger function (migration `0001`). Keeps
  a row's `updated_at` current on any `UPDATE`. Shared by every table
  that wants this behavior going forward, not just `transactions` —
  a later migration attaches the same trigger function to a new table
  rather than redefining the logic.

## Not yet in this schema

Task 56/d (a through e) is fully built. Task 57/c (`customers` table +
RLS, migrations `0003`/`0004`) is built as of this update — see that
table's own entry above. Still open: Task 57/d (vault read/write logic
— resolving `customer_id` into these columns, `save_customer: true`
handling, returning the new `customer_id`) and Task 57/e (wiring
`customer_id`/`save_customer` into the live `/pay` handler). Until (d)
lands, this table exists but nothing in the running application reads
from or writes to it yet. Beyond that, nothing currently queued needs
a further migration; the next schema change is whatever a future task
actually requires (e.g. Task 46's dashboard, once it needs a
`businesses` table or per-business RLS).
