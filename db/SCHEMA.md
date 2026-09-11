# Schema — running summary

Source of truth is `db/migrations/` (sequentially numbered, append-only
— see Task 56/b in `handover.md` for the full convention). This file
is a short, current index so a session doesn't have to reconstruct the
schema by reading every migration in order. **Update this file in the
same session as any migration that changes it.**

**Migrations `0001`, `0003`, `0004`, `0016`, `0017`, `0019`, `0020`,
`0021`, and `0022` are confirmed live** — `0001` applied by the product owner via `psql -f`
(2026-09-08), from the second (proot-distro Ubuntu) environment,
against project ref `mfekzzwsoiezqkovabmp`; `0003`/`0004` applied the
same way, same session (`CREATE TABLE` / `CREATE TRIGGER` / `ALTER
TABLE` / `CREATE POLICY` all confirmed — the `DROP TRIGGER IF EXISTS`
"does not exist, skipping" notice is expected on a brand-new table,
same as `0001` saw for `transactions`); `0016`/`0017` applied the same
way, 2026-09-10, via `\i` at the live `psql` prompt (`CREATE TABLE` /
`CREATE TRIGGER` / `CREATE INDEX` / `ALTER TABLE` / `CREATE POLICY`
all confirmed, no errors); `0019`/`0020` applied the same way,
2026-09-11 (product-owner-reported to this session as deployed — this
session has no network path to the live Supabase project from its own
sandbox to independently re-run `\dt`/`\d routing_fallbacks` itself,
same limitation as every prior migration recorded in this file; taken
at the product owner's own word, same as every entry above it).
`0021`/`0022` applied the same way, 2026-09-11, via `\i` at the live
`psql` prompt against a Postgres 17 server (`CREATE TABLE` /
`CREATE INDEX` / `ALTER TABLE` / `CREATE POLICY` all confirmed, no
errors — product-owner-reported to this session the same way `0019`/
`0020` were, same limitation on independent re-verification). Every
migration in `db/migrations/` not
listed here still needs its own confirmation the same way before this
file should be treated as describing live state for it — check
`handover.md`'s per-migration notes, or run `\dt`/`\d <table>`
yourself, rather than assuming everything in this directory has been
applied just because some of it has. Migration `0002` (transactions
RLS) is not yet confirmed live as of this update.

## Shared conventions (locked in by migration `0001`, followed by every migration after)

- **id**: `uuid`, `default gen_random_uuid()` (via the `pgcrypto` extension)
- **timestamps**: `created_at` / `updated_at`, both `timestamptz`, `default now()` — `updated_at` kept current via the shared `set_updated_at()` trigger function, not per-table logic
- **status columns**: `text` + a `CHECK` constraint listing the allowed values, not a Postgres `enum` — easier for a later migration to extend the allowed list than an enum's `ALTER TYPE ... ADD VALUE`
- **no placeholder foreign keys**: a table doesn't get an FK to a table that doesn't exist yet just because a future task is expected to add one

## Tables

### `transactions` (migration `0001`, `provider_reference` added by `0009`)

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `reference` | `text` | unique (see index below) — what `GET /payout/verify` looks up by (Task 56/d-4) |
| `type` | `text` | `'payment'` \| `'payout'` — both `POST /pay` and `POST /payout` write here (Task 56/d-3) |
| `provider` | `text` | |
| `currency` | `text` | |
| `amount` | `numeric` | |
| `status` | `text` | `'pending'` \| `'success'` \| `'failed'`, default `'pending'` |
| `provider_reference` | `text` | added by migration `0009` (Task 45d) — nullable, no default. Holds a provider-issued id when that provider's own verify call needs it instead of the merchant `reference` (JuicyWay's `GET /payments/{id}` today). Deliberately generic, not `juicyway_payment_id` — Flutterwave's own analogous `verifyPayout` gap (see Task 52/d-2a's own note: "callers must pass `data.id` from `processPayout()`'s own response") can reuse this same column later without another migration. `null`/absent for every provider whose own verify call already accepts the merchant reference directly (Paystack, Korapay, Flutterwave-v3-collection) — this is the exception path, not the common one. |
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

### `capabilities` (migration `0007`) — Task 52/e-2e's Stripe-precedent capability-status table

| Column | Type | Notes |
|---|---|---|
| `capability` | `text` | primary key — one row per named capability (`collection`, `payout`, `banks`, `kyc`, `card_issuance`, `gift_cards`, `vtu`) |
| `status` | `text` | `'active'` \| `'pending'` \| `'not_implemented'`, default `'not_implemented'` |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` trigger — flipping a capability to `active` (the moment its real route lands) leaves an auditable timestamp |

**Seeded** (same migration) with today's REAL status per capability,
not aspirational: `collection`/`payout`/`banks` → `active` (`POST
/pay`/`POST /payout`/`GET /banks` are live, working routes today);
`kyc`/`card_issuance`/`gift_cards`/`vtu` → `not_implemented` (Tasks
53/54/48/44 are each still decision-record only — no actual route for
any of them exists in this codebase yet).

**Purpose:** resolves Task 52/e-2e's "not buildable yet, no concrete
route exists" gap by mirroring Stripe's Capabilities API — each
capability (card_payments, transfers, treasury, ...) on a Stripe
Account is tracked as its own independent entity with its own status,
long before every requirement behind it is satisfied. A capability-mix
flow (Task 55/b) can check a capability's status and degrade cleanly
(a 501) instead of assuming it exists — see handover.md's Task 52/e-2e
entry for the full precedent writeup and decision record.

**No foreign keys.** Nothing else in this repo references
`capabilities` rows.

**Row Level Security:** enabled (migration `0008`). One explicit
policy, `capabilities_service_role_all`, scoped to `service_role`
only — same pattern as every other table in this schema. No policy
for `anon`/`authenticated`.

**Read path wired; no write-triggering caller yet.**
`utils/supabase.js`'s `getCapabilityStatus(capability)` (never
throws, same posture as every other helper in that file) and
`routes.js`'s `assertCapabilityActive(capability)` (throws a 501 for
anything other than `'active'`, including an unknown/missing status —
fails closed, doesn't guess) both exist and are verified, but neither
is called from any route yet — `collection`/`payout`/`banks` are
already unconditionally active in practice (their routes exist and
work), so there's nothing to usefully gate on those paths today.
`assertCapabilityActive()` is built ready for whichever future route
Task 53/54 adds (`/kyc`, a card-issuance endpoint, etc.) to call
before doing anything else.

### `businesses` (migration `0010`) — Task 58/c's per-business account, pulled forward from Task 45/b

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `email` | `text` | `not null unique` — B-Pay account identity, distinct from any `telcos.opik.net` credential (see `api_keys` below) |
| `company_name` | `text` | nullable |
| `status` | `text` | `'active'` \| `'suspended'`, default `'active'` — no CHECK constraint yet, see migration `0010`'s own note |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` |

**No foreign keys into this table yet from `transactions`/`customers`** — flagged as separate follow-up scope in migration `0010`, not done here (mirrors migration `0001`'s own original "no `business_id` FK until a `businesses` table exists" deferral, now resolved by this table existing but not yet wired to those two).

**Row Level Security:** enabled (migration `0011`). One explicit policy, `businesses_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema. No policy for `anon`/`authenticated` — no dashboard read path exists yet (Task 46), and Task 45/c's dashboard-login-vs-API-key question is still open.

**Not yet built:** no dashboard-login credential column (deliberately — see migration `0010`), no admin/role column, no `business_id` FK on `transactions`/`customers`.

### `api_keys` (migration `0012`) — Task 58/c-2/c-3's per-business, per-provider credential store

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `business_id` | `uuid` | `not null references businesses(id) on delete cascade` |
| `provider` | `text` | e.g. `'telcosopik'` — generic column, not scoped to one provider |
| `vault_secret_id` | `uuid` | reference into Supabase Vault (`vault.secrets.id`) — **the raw key is never stored in this table**, see migration `0012`'s own note |
| `key_prefix` | `text` | nullable, non-secret display fragment (e.g. `sk_live_...a1b2`) |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` |

**Constraint:** `unique (business_id, provider)` — the DB-level half of Task 58/c-3's duplicate-registration guard (check-then-create at the application layer, this constraint as the actual enforcement point against a lost race). Indexed on `business_id`.

**Encryption at rest (Task 58/c-2):** Supabase Vault, mirroring Stripe's own envelope-encryption pattern for stored secrets. Insert via `select vault.create_secret(<raw key>, <name>, <description>)`, store the returned id here. Read the real value back only via an explicit `select decrypted_secret from vault.decrypted_secrets where id = <vault_secret_id>` — never logged, never returned in a list/display response (only `key_prefix` is safe to show).

**Row Level Security:** enabled (migration `0013`). One explicit policy, `api_keys_service_role_all`, scoped to `service_role` only. No policy for `anon`/`authenticated` — see migration `0013`'s own note on why even a future dashboard read path should go through a server-side endpoint rather than a direct RLS-scoped client read.

**Not yet built:** the actual account-provisioning call site (`providers/telcosOpik.js`'s constructor / `getProviderKey('telcosopik', businessId)`, Task 58/c, order-of-execution step 4) — this migration only creates the storage, same division of labor migration `0003` used for `customers`.

### `balance_transactions` (migration `0014`) — Task 61/a's append-only ledger, Stripe `BalanceTransaction`-shaped

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `business_id` | `uuid` | nullable, `references businesses(id)` — populated where the write path knows one (VTU routes); `/pay`/`/payout` can't yet, since `transactions.business_id` itself doesn't exist |
| `transaction_id` | `uuid` | nullable, `references transactions(id)` — link back to the attempt-level record, when one exists |
| `reference` | `text` | nullable, plain correlation column — deliberately **not** an FK to `transactions.reference` (see migration `0014`'s own note on why) |
| `provider` | `text` | `not null` |
| `type` | `text` | `'payment'` \| `'payout'` \| `'fee'` \| `'refund'` \| `'adjustment'` — `'fee'`/`'refund'` have no write path yet (Task 63/d still blocked on its own discovery pass), included now so widening the list later needs no new migration |
| `amount` | `numeric` | |
| `currency` | `text` | |
| `available_on` | `timestamptz` | nullable — mirrors Stripe's pending-vs-available split; `null` = available immediately, today's honest default since no per-provider settlement delay is currently confirmed |
| `created_at` | `timestamptz` | default `now()` — **no `updated_at`, deliberately** |

**Append-only — the one table in this schema that does NOT get the shared `updated_at`/`set_updated_at()` treatment.** A ledger row is a statement of fact; a correction is a new row (e.g. a refund is its own `type: 'refund'` row referencing the original payment), never an in-place edit of an old one. No application code should ever `UPDATE` this table.

**Indexes:** `balance_transactions_business_id_idx`, `balance_transactions_transaction_id_idx`, `balance_transactions_reference_idx` — one per the three lookup patterns this table currently serves (per-business, per-attempt, per-reference). No `created_at` index yet — added later if a reconciliation job's real query shape (Task 61/d, still open) needs one.

**Row Level Security:** enabled (migration `0015`). One explicit policy, `balance_transactions_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema.

**Not yet built:** nothing writes to this table yet (Task 61/b — wiring `/pay`/`/payout`/VTU routes to insert a row alongside their existing `recordTransaction()` call — is still open), no per-business balance view reads from it yet (Task 61/c), and no reconciliation job exists (Task 61/d, blocked on a still-needed per-provider discovery pass for whether each provider exposes a statement/settlement endpoint to reconcile against).

- **`set_updated_at()`** — trigger function (migration `0001`). Keeps
  a row's `updated_at` current on any `UPDATE`. Shared by every table
  that wants this behavior going forward, not just `transactions` —
  a later migration attaches the same trigger function to a new table
  rather than redefining the logic.

### `webhook_events` (migrations `0016`/`0017`) — Task 60/a's queryable delivery-record table

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `provider` | `text` | `not null` — no `CHECK` list, same as `balance_transactions.provider` |
| `provider_event_id` | `text` | nullable — B-Pay's own column name; the actual field each provider's payload uses is still unmapped per provider (Task 60/c, not started) |
| `payload` | `jsonb` | `not null` — the full raw webhook body |
| `signature_valid` | `boolean` | `not null` — recorded for every delivery, including failed verification |
| `status` | `text` | `'received'` \| `'processed'` \| `'failed'`, default `'received'` — a real in-place lifecycle column, unlike `balance_transactions.type` |
| `received_at` | `timestamptz` | default `now()` |
| `processed_at` | `timestamptz` | nullable — set once `status` leaves `'received'` |
| `updated_at` | `timestamptz` | default `now()`, kept current by the shared `set_updated_at()` trigger — this table follows the shared convention (unlike `balance_transactions`) since `status` is a genuine lifecycle |

**Indexes:** `webhook_events_provider_event_idx` on `(provider, provider_event_id)`, for Task 60/b's dedup lookup. `webhook_events_provider_status_idx` on `(provider, received_at desc, status)` (migration `0018`, Task 60/e) — serves `getRecentWebhookOutcomes()`'s "most recent N rows for one provider" query, used to detect a consecutive-failure streak.

**Row Level Security:** enabled (migration `0017`). One explicit policy, `webhook_events_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema.

**Status as of Task 60/e (2026-09-10):** a/b/c/d/e all done — dedup wired in `routes.js`'s `webhookHandlers` (60/b), per-provider event-id field discovery closed with Paystack/Korapay confirmed and JuicyWay flagged as a real open question (60/c), manual replay route live (60/d), and continuous-failure alerting wired via `utils/alerts.js`'s channel-agnostic `notifyOps()` (60/e) — see Task 60/e's own section in handover.md.

### `routing_fallbacks` (migrations `0019`/`0020`) — Task 63/a's machine-readable ordered fallback chain per domain

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `domain` | `text` | `not null` — one of `classifyDomain()`'s two return values (`'african_rails'` \| `'international'`); no `CHECK`, same reasoning as `routing_config.default_provider` |
| `provider` | `text` | `not null` — no `CHECK`, `getProvider()` validates at request time |
| `priority` | `integer` | `not null` — `0` = current default, `1`/`2`/`3` = fallback order, lower attempted first |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via the shared `set_updated_at()` trigger |

**Constraints:** unique `(domain, priority)` (no ambiguity about which provider is "next"), unique `(domain, provider)` (a provider can't appear twice in one domain's chain).

**Seeded** (same migration) with Task 51's b-1/b-2 fallback-order tables exactly as they stand in `handover.md` today (international: juicyway → korapay → paystack → flutterwave; african_rails: korapay → paystack → juicyway → flutterwave) — applying this migration changes no current routing decision, only makes an already-agreed one queryable.

**Not yet wired into the running application.** This leaf is data only — nothing in `routes.js` reads this table yet; that's Task 63/b's own scope (the actual fallback-attempt logic).

**Known, flagged tradeoff, not resolved this leaf:** this table's own `priority = 0` row and `routing_config.default_provider` (above) both encode "today's default provider for domain X." Promoting a fallback to default today only updates `routing_config` (migration `0005`'s own documented workflow) — it does **not** keep this table in sync, so the two can drift if one is edited without the other. Reconciling that (e.g. Task 63/b deriving the default from this table's own `priority = 0` row instead of reading `routing_config` separately) is left for whoever builds Task 63/b, flagged here so it isn't rediscovered as a surprise.

**Row Level Security:** enabled (migration `0020`). One explicit policy, `routing_fallbacks_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema.

### `idempotency_keys` (migrations `0021`/`0022`) — Task 65/a+b's caller-supplied idempotency-key cache

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `business_id` | `text` | `not null` — plain caller-supplied string, same posture as `getVtuBusinessId()` (routes.js) — NOT `uuid references businesses(id)`, since that function has no format/FK validation either |
| `idempotency_key` | `text` | `not null` — the caller-supplied `Idempotency-Key` header value |
| `request_hash` | `text` | `not null` — sha256 of a stable (sorted-key) JSON serialization of the request body (`hashRequestBody()`, utils/helpers.js); stored for Task 65/c's future use, not compared anywhere yet |
| `response_status` | `integer` | `not null` |
| `response_body` | `jsonb` | `not null` — the exact response replayed verbatim on a cache hit |
| `created_at` | `timestamptz` | default `now()` — no `updated_at`/trigger, same reasoning as `balance_transactions`: insert-once, never updated |

**Constraints:** unique `(business_id, idempotency_key)`.

**Scope, narrower than Task 65/a's own text:** wired into the two VTU purchase routes only (`POST /vtu/data`, `POST /vtu/airtime`, via `idempotencyCache()` in `routes.js`) — **not** `/pay`/`/payout`. Checked first: those two routes have no `business_id` in scope at all today (their only caller is the internal Edge Function via a single shared `requireInternalApiKey` secret), so there's nothing correct to key this table's cache on for them yet. Flagged as a real, separate gap in Task 65's own write-up (handover.md), touching similar ground to Task 72's open questions.

**Behavior:** no `Idempotency-Key` header → route behaves exactly as before (opt-in). A previously-seen `(business_id, key)` → the original cached response is replayed verbatim, the provider is never called again. A `5xx` response is never cached — only a completed request (successful or a clean, provider-communicated failure) is treated as a stable, safe-to-replay outcome.

**Not yet built:** Task 65/c (what to do when a reused key comes with a genuinely different request body — `request_hash` exists for exactly this, unused so far) and Task 65/d (documenting which failure classes aren't idempotency-safe) are both still open.

**Row Level Security:** enabled (migration `0022`). One explicit policy, `idempotency_keys_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema.

### `payment_intents` (migrations `0023`/`0024`) — Task 72's stable, caller-facing handle

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `business_id` | `uuid` | nullable, `references businesses(id)` — see migration `0023`'s own note on why this table uses the `uuid`/FK convention (`balance_transactions`), not the plain-`text` convention (`idempotency_keys`) |
| `reference` | `text` | `not null`, unique — the stable, caller-facing handle; same value the caller already supplies today |
| `type` | `text` | `'payment'` \| `'payout'`, mirrors `transactions.type` |
| `currency` | `text` | |
| `amount` | `numeric` | |
| `status` | `text` | `'pending'` \| `'success'` \| `'failed'`, default `'pending'` — aggregate across this intent's `payment_attempts`; the aggregation logic itself is not built here |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` trigger |

**Indexes:** `payment_intents_reference_key` (unique, on `reference`), `payment_intents_business_id_idx`.

**Purpose:** resolves Task 72's confirmed proposal — mirrors Stripe's `PaymentIntent`. `transactions.reference` currently does double duty as both the caller-facing handle and the per-provider value forwarded on each attempt, which blocks Task 63/b's cross-processor retry from giving a caller one stable handle across attempts against different providers. This table is that stable handle; `payment_attempts` below is where each individual attempt's own provider-facing reference lives.

**Not duplicated here:** `amount`/`currency` are not repeated per-attempt on `payment_attempts` — no current scenario in this repo has a retry change the requested amount; add it there if that need is ever confirmed, not guessed at now.

**Row Level Security:** enabled (migration `0024`). One explicit policy, `payment_intents_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema.

**Not yet wired into the running application.** Nothing in `routes.js` creates or reads a `payment_intents` row yet — that's Task 63/b's own scope, same "storage only" division of labor every prior create-table migration in this schema has used.

### `payment_attempts` (migrations `0025`/`0026`) — Task 72's per-attempt record, Stripe `PaymentRecord`-shaped

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key |
| `payment_intent_id` | `uuid` | `not null references payment_intents(id)` |
| `provider` | `text` | `not null` |
| `reference` | `text` | `not null`, unique — the value actually forwarded to this specific attempt's provider; deliberately cannot reuse `payment_intents.reference` (that's the entire problem this table exists to solve) |
| `provider_reference` | `text` | nullable — mirrors `transactions.provider_reference` (migration `0009`) exactly |
| `status` | `text` | `'pending'` \| `'success'` \| `'failed'`, default `'pending'` |
| `error_type` / `error_code` / `error_decline_code` / `error_param` | `text`, all nullable | Task 62's confirmed `ApiError` taxonomy (`ERROR_HANDLING.md`) — stored per-attempt, not yet populated by anything (Task 62/e is still open), internal-only, same as `ApiError`'s own fields |
| `created_at` | `timestamptz` | default `now()` |
| `updated_at` | `timestamptz` | default `now()`, auto-updated via `set_updated_at()` trigger |

**Indexes:** `payment_attempts_reference_key` (unique, on `reference`), `payment_attempts_payment_intent_id_idx`.

**Real, unresolved overlap with `transactions` — flagged, not decided here:** `transactions` (migration `0001`) already records one row per actual provider call today, which is structurally what this table also does. This migration does not touch `transactions`, migrate its data, or decide how the two relate going forward. That reconciliation is Task 63/b's own scope — see migration `0025`'s own header comment for the full reasoning on why this wasn't decided unilaterally here.

**Row Level Security:** enabled (migration `0026`). One explicit policy, `payment_attempts_service_role_all`, scoped to `service_role` only — same pattern as every other table in this schema.

**Not yet wired into the running application.** Same as `payment_intents` above — no route creates or reads a row here yet; Task 63/b's scope.

## Not yet in this schema

Task 56/d (a through e) is fully built. Task 57 (a through e,
`customers` table + Customer Vault) is fully built. Task 52/e-2d
(`routing_config`, migrations `0005`/`0006`), Task 52/e-2e
(`capabilities`, migrations `0007`/`0008`), Task 45d
(`transactions.provider_reference`, migration `0009`), and Task 58/c
(`businesses`/`api_keys`, migrations `0010`–`0013`) are all built and
wired in as of this update — see each table's own entry above.
**Not yet built for `routing_config`:** a second, per-account/per-
domain override tier beyond the single platform-level default row per
domain — flagged as a future extension of that table, not guessed at
in migration `0005`. **Not yet built for `capabilities`:** any route
actually calling `assertCapabilityActive()` — it exists, verified,
ready, with nothing to call it yet (Tasks 53/54 are still decision-
record only). **Not yet built for `provider_reference`:** the same
column populated for Flutterwave's own analogous `verifyPayout` gap —
flagged as a future reuse of this column, not built here (Task 45d
scoped to JuicyWay's collection-verify gap specifically). **Not yet
built for `businesses`/`api_keys`:** the actual `telcosOpik.js`
provisioning code that reads/writes these tables (Task 58's own
order-of-execution step 4, not this migration); a `business_id` FK on
`transactions`/`customers`; any dashboard-login credential (Task
45/c's still-open question); the actual Vault `create_secret`/
`decrypted_secrets` call sites. **Not yet built for
`balance_transactions` (migrations `0014`/`0015`, Task 61/a):**
anything that writes to it (Task 61/b), any per-business balance view
reading from it (Task 61/c), and any reconciliation job (Task 61/d) —
this migration is storage only, same division of labor every prior
create-table migration in this schema used. Migrations `0005` through
`0015` are **not yet confirmed live** — same "check before assuming"
caveat this file's own top note already states for every migration
not explicitly listed as confirmed there (`0016`/`0017` are now
confirmed, see the top note). **Not yet built for
`webhook_events` (migrations `0016`/`0017`, Task 60/a):** anything
that writes to it (Task 60/b), any per-provider `provider_event_id`
mapping (Task 60/c), any manual replay route (Task 60/d), and any
failure alerting (Task 60/e, blocked on a product-owner decision) —
this migration is storage only, same division of labor every prior
create-table migration in this schema used. Beyond that, nothing
currently queued needs a further migration; the next schema change is
whatever a future task actually requires (e.g. Task 46's dashboard,
once its own auth design is decided, Task 61/b, or Task 60/b, once a
session picks one of them up). **Not yet built for `payment_intents`/
`payment_attempts` (migrations `0023`–`0026`, Task 72):** anything
that writes to or reads from either table (Task 63/b's own scope),
and the unresolved `payment_attempts`/`transactions` overlap flagged
in migration `0025`'s own header — this migration pair is storage
only, same division of labor as every prior create-table migration in
this schema. Migrations `0023`–`0026` are **not yet confirmed live** —
same "check before assuming" caveat this file's own top note already
states for every migration not explicitly listed there.
