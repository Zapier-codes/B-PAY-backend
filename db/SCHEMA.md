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

### `routing_config` (migration `0005`) — Task 52/e-2d's Stripe-precedent default-provider table

| Column | Type | Notes |
|---|---|---|
| `domain` | `text` | primary key — one row per `classifyDomain()` return value (`'african_rails'` \| `'international'`) |
| `default_provider` | `text` | no `CHECK` constraint (see migration's own note — routes.js's `getProvider()` validates this at request time instead) |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` trigger — this is what makes a "promote a fallback to default" `UPDATE` auditable with no application code |

**Seeded** (same migration) with today's live defaults —
`african_rails` → `korapay`, `international` → `juicyway` — exactly
matching routes.js's own `DOMAIN_DEFAULT_PROVIDER` fallback table, so
applying this migration changes no current routing behavior.

**Purpose:** resolves Task 52/e-2d's open design question (env var vs.
config file vs. admin-dashboard toggle) by mirroring Stripe's actual
Payment Method Configurations model — a live, Dashboard-toggleable,
API-backed object, not a deploy. Until Task 46's real admin dashboard
exists, the product owner edits this table's rows directly (SQL, from
the DB-Ops second environment) as their "dashboard" — see handover.md's
Task 52/e-2d entry for the full precedent writeup.

**No foreign keys.** Nothing else in this repo references
`routing_config` rows.

**Row Level Security:** enabled (migration `0006`). One explicit
policy, `routing_config_service_role_all`, scoped to `service_role`
only — same pattern as `transactions`/`customers` above, same
reasoning, with an extra edge here specifically: an open
`anon`/`authenticated` write policy on this table could silently
redirect real payment traffic to a different provider. No policy for
`anon`/`authenticated`.

**Wired into the running application as of this update** —
`routes.js`'s `resolveDomainDefaultProvider(domain)` (Task 52/e-2d)
reads this table first at all four call sites that previously read
`DOMAIN_DEFAULT_PROVIDER[domain]` directly (`POST /pay`, `POST
/payout`, `GET /payout/verify`, `GET /banks`), falling back to the
hardcoded table on any miss (not configured, table not yet migrated on
this environment, or a domain with no row) — same never-fail-the-
request posture every other Supabase-backed lookup in this file
already uses.

## Functions

- **`set_updated_at()`** — trigger function (migration `0001`). Keeps
  a row's `updated_at` current on any `UPDATE`. Shared by every table
  that wants this behavior going forward, not just `transactions` —
  a later migration attaches the same trigger function to a new table
  rather than redefining the logic.

## Not yet in this schema

Task 56/d (a through e) is fully built. Task 57 (a through e,
`customers` table + Customer Vault) is fully built as of this file's
own prior update. Task 52/e-2d (`routing_config` table, migrations
`0005`/`0006`) is built and wired in as of this update — see that
table's own entry above. **Not yet built for `routing_config`:** a
second, per-account/per-domain override tier beyond the single
platform-level default row per domain (Stripe's own Configurations
model supports this via a specific config ID referenced per Checkout
Session; this table doesn't yet have an equivalent scope column) —
flagged as a future extension of this same table, not guessed at in
migration `0005`. Migrations `0005`/`0006` themselves are **not yet
confirmed live** — same "check before assuming" caveat this file's
own top note already states for every migration not explicitly listed
as confirmed there. Beyond that, nothing currently queued needs a
further migration; the next schema change is whatever a future task
actually requires (e.g. Task 46's dashboard, once it needs a
`businesses` table or per-business RLS).
