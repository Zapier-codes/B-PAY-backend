-- Migration 0021: create the `idempotency_keys` table.
--
-- Task 65/a+b — accepts a caller-supplied `Idempotency-Key` header and
-- caches the resulting response, keyed per business, mirroring
-- Stripe's own real behavior (confirmed against docs.stripe.com this
-- session): a repeated request with the same key returns the original
-- cached response instead of re-calling the provider.
--
-- **Scope, deliberately narrower than Task 65/a's own text.** That
-- leaf lists `/pay`, `/payout`, and the VTU purchase routes. Checked
-- first, per this repo's own "confirm before building" discipline:
-- `/pay` and `/payout` have NO `business_id` in scope at all today —
-- `recordBalanceTransaction()`'s own call sites in `routes.js`
-- explicitly leave it unset (see those routes' own Task 61/b
-- comments), because their only caller is the internal Supabase Edge
-- Function via a single shared `requireInternalApiKey` secret, not a
-- per-business credential. Only the two VTU purchase routes
-- (`POST /vtu/data`, `POST /vtu/airtime`) have a real, caller-supplied
-- `businessId` today (`getVtuBusinessId()`, decided 2026-09-09). This
-- migration and its wiring cover those two routes only. Extending
-- this to `/pay`/`/payout` needs a `business_id` source for them
-- first — a real, separate gap, not invented here (see Task 72's own
-- open questions, which touch the same underlying "what identifies
-- the caller of `/pay`/`/payout`" territory).
--
-- **`business_id` is `text`, not `uuid references businesses(id)`,
-- unlike `api_keys`/`balance_transactions`.** `getVtuBusinessId()`
-- treats it as "a plain, required, caller-supplied field" with no
-- format validation and no FK check against the `businesses` table
-- (that function's own header comment) — this column matches that
-- exact, already-established posture rather than silently imposing a
-- stricter UUID/FK constraint the rest of this codebase doesn't
-- enforce yet.
--
-- **`request_hash` is stored but NOT enforced/compared in this
-- migration or its wiring.** Task 65/c (deciding what happens when
-- the same key is reused with a genuinely different request body) is
-- explicitly its own still-open decision, flagged for product-owner
-- confirmation rather than assumed. Storing the hash now — without
-- acting on it — means that decision can be implemented later without
-- another migration.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

create table if not exists idempotency_keys (
  id uuid primary key default gen_random_uuid(),

  -- Plain caller-supplied string, same posture as `getVtuBusinessId()`
  -- — see this file's own header note above.
  business_id text not null,

  -- The caller-supplied `Idempotency-Key` header value.
  idempotency_key text not null,

  -- sha256 of a stable (sorted-key) JSON serialization of the request
  -- body — stored for Task 65/c's future use, not compared here.
  request_hash text not null,

  -- The exact response this route sent the first time, replayed
  -- verbatim on a cache hit rather than re-calling the provider.
  response_status integer not null,
  response_body jsonb not null,

  -- No `updated_at`/trigger — same reasoning as `balance_transactions`
  -- (migration 0014): this table is insert-once, a row is never
  -- updated after it's written.
  created_at timestamptz not null default now(),

  -- One cached response per (business, key) — a second insert attempt
  -- for the same pair is exactly the "already cached, don't
  -- duplicate" case this table exists to serve.
  constraint idempotency_keys_business_key_unique unique (business_id, idempotency_key)
);

create index if not exists idempotency_keys_lookup_idx
  on idempotency_keys (business_id, idempotency_key);
