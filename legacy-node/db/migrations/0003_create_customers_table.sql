-- Migration 0003: create the `customers` table (the "Customer Vault",
-- Task 57's Piece 2).
--
-- Per Task 57/c: mirrors migration 0001's own conventions (see
-- db/SCHEMA.md) — same id/timestamp/status-column style, same
-- Patch Handoff discipline. New table, first one since `transactions`
-- (migration 0001).
--
-- Scope, per Task 57's own top-level writeup: this table stores only
-- the DURABLE part of a customer's profile — the part reusable across
-- many future transactions, not anything specific to one purchase.
-- Task 57 is explicit that "order.identifier", "order.items", and
-- "description" are per-transaction and must never be vaulted here;
-- this migration only adds columns for the fields Task 57's own
-- writeup names as durable: name, phone, billing address, customer
-- type. Reading/writing this table (resolving a `customer_id` into
-- these fields, handling `save_customer: true`, returning the new
-- `customer_id`) is Task 57/d's job, not this migration — this part
-- only creates the storage.
--
-- Deliberately NOT included, flagged rather than silently omitted:
--   - `email`: per Task 57's own Piece 1 writeup, the canonical
--     `customer.email` is supplied on every call and always wins over
--     anything else — it is never vaulted, so there's no column for
--     it here.
--   - `ip_address`: JuicyWay's own required customer field (see
--     utils/fieldRequirements.js's `juicyway` entry,
--     `provider_data.juicyway.customer.ip_address`) is deliberately
--     NOT included as a vaultable column. Task 57's own "important
--     nuance" paragraph lists exactly four durable customer fields —
--     name, phone, billing address, customer type — and IP address
--     isn't one of them: it's the caller's network location *at
--     request time*, not a durable attribute of the customer, so
--     vaulting it would mean serving a stale (or simply wrong, if the
--     customer is transacting from a different network next time)
--     value on a future call. Same "don't vault per-transaction
--     context" principle Task 57 already applies to `order`/
--     `description`, just for a request-context reason rather than a
--     per-purchase-content one. `ip_address` still gets supplied
--     fresh, per call, via `provider_data.juicyway.customer.ip_address`
--     same as today — this table gives it nowhere to live.
--
-- Column naming note: JuicyWay's own field is `customer.type`
-- (individual vs. business, per utils/fieldRequirements.js) — named
-- `customer_type` here instead of `type`, to avoid any confusion with
-- `transactions.type` (`payment` vs. `payout`, migration 0001), which
-- means something entirely different despite the same bare word.
--
-- No CHECK constraint on `customer_type`: unlike `transactions.status`/
-- `transactions.type` (migration 0001), whose allowed values were
-- confirmed against provider docs before the CHECK was written, this
-- session has not independently confirmed JuicyWay's full allowed set
-- for `customer.type` beyond seeing it referenced as a required string
-- field — left as unconstrained `text` rather than guessing a specific
-- value list, flagged here as an open item for whoever next touches
-- this column with a confirmed list in hand.
--
-- Every column below is nullable (no `not null`, aside from `id` and
-- the timestamps) — this table is a general-purpose vault, not scoped
-- to one provider's exact requirement set, so it must be able to hold
-- a partially-populated profile (e.g. a caller who's only supplied a
-- name so far) without a NOT NULL constraint rejecting the insert.
-- Task 57/d's own resolution-order logic (request field -> vaulted
-- row -> 400 naming the still-missing field) is what actually
-- enforces "required for this provider", not this table.
--
-- No foreign keys yet, per the "no placeholder FKs" convention
-- (db/SCHEMA.md) — nothing in this repo currently references
-- `customers.id` from another table.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

create table if not exists customers (
  id uuid primary key default gen_random_uuid(),

  first_name text,
  last_name text,
  phone_number text,
  billing_address text,
  customer_type text,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

-- Reuses the shared `set_updated_at()` trigger function created in
-- migration 0001 — same convention as `transactions`, not a
-- per-table copy of the logic.
drop trigger if exists customers_set_updated_at on customers;

create trigger customers_set_updated_at
before update on customers
for each row
execute function set_updated_at();
