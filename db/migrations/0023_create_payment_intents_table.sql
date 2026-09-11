-- Migration 0023: create the `payment_intents` table.
--
-- Task 72 — closes STRIPE_DISCOVERY.md's gap #1. Builds the first of
-- the two additive, product-owner-confirmed objects from Task 72's
-- proposal, now that all three of that leaf's open questions have an
-- answer (see handover.md's Task 72 section for the full confirmed
-- decision record; summarized here only as it bears on this table):
--
--   1. Additive, not a replace/rename of `transactions` — matches
--      Stripe's own precedent directly (`PaymentRecord` sits next to
--      `PaymentIntent`, it doesn't replace it). This migration does
--      NOT touch `transactions` at all.
--   2. Non-breaking to the existing caller (the Supabase Edge
--      Function, per Task 23's own audit) — this table's `reference`
--      column is the same stable, caller-facing value the caller
--      already supplies today; nothing about what the caller sends
--      changes because this table now exists.
--   3. Sequencing — this (the stable handle) is built first; Task
--      63/b's cross-processor retry logic depends on it existing,
--      per this session's direct instruction.
--
-- **The concrete problem this solves (Task 72's own writeup):**
-- `transactions.reference` (migration `0001`) is `UNIQUE` table-wide
-- and currently does two jobs at once — the caller-facing handle
-- *and* the exact value forwarded to one specific provider. A
-- cross-processor retry (Task 63/b) needs a *second* provider-facing
-- reference for the second attempt, which the existing single column
-- can't hold without either violating its own uniqueness or
-- overwriting the first attempt's row. This table is the stable
-- handle; migration `0025`'s `payment_attempts` table is where each
-- individual attempt's own provider-facing reference actually lives.
--
-- **`business_id` — `uuid references businesses(id)`, nullable. A
-- deliberate choice, not the only convention this schema already
-- uses — flagged explicitly rather than picked silently:** migration
-- `0021`'s `idempotency_keys.business_id` is a plain, unvalidated
-- `text` column, because that table is scoped narrowly to the two
-- VTU routes and mirrors `getVtuBusinessId()`'s own already-decided
-- posture for exactly those two routes. `payment_intents` is meant to
-- eventually be the general object across `/pay`, `/payout`, *and*
-- the VTU routes (Task 72's own scope, unlike Task 65's VTU-only
-- scope) — so this table follows `balance_transactions`
-- (migration `0014`)'s stricter `uuid references businesses(id)`
-- convention instead. Nullable for the same reason
-- `balance_transactions.business_id` is nullable: `/pay`/`/payout`
-- have no `business_id` in scope at all today (same gap Task 65/a+b's
-- own writeup already names) — this column exists now so wiring
-- those two routes in later needs no further migration, but it is
-- not populated by anything yet.
--
-- **`type`/`currency`/`amount` mirror `transactions`'s own columns
-- exactly (migration `0001`)** — no new convention invented. `status`
-- uses the same three-value vocabulary (`'pending'`/`'success'`/
-- `'failed'`) as `transactions.status`, not Stripe's own
-- `'succeeded'` — chosen for internal consistency with the rest of
-- this schema over literal Stripe terminology, since this column's
-- value is meant to be an aggregate over this intent's own
-- `payment_attempts` rows using the same vocabulary those rows (and
-- `transactions`) already use. **The actual aggregation logic (how
-- multiple attempt statuses roll up into one intent status) is
-- deliberately not built here** — that's application code, Task
-- 63/b's own scope, same "storage only, wiring is a later task's job"
-- division of labor every prior create-table migration in this
-- schema has used.
--
-- **Not duplicated here, flagged rather than silently omitted:**
-- `amount`/`currency` live on this table only, not repeated per-row
-- on `payment_attempts` — this repo has no current scenario where a
-- retry changes the requested amount, so a second, potentially-
-- inconsistent copy on the attempt table isn't built until an actual
-- need for it is confirmed, not guessed at now.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

create table if not exists payment_intents (
  id uuid primary key default gen_random_uuid(),

  business_id uuid references businesses(id),

  -- The stable, caller-facing handle — same value the caller (the
  -- Supabase Edge Function) already supplies today. Unique, same
  -- posture as `transactions.reference` (migration 0001).
  reference text not null,

  type text not null check (type in ('payment', 'payout')),

  currency text not null,
  amount numeric not null,

  -- Aggregate status across this intent's attempts. See this file's
  -- own header note: the aggregation logic itself is not built here.
  status text not null default 'pending'
    check (status in ('pending', 'success', 'failed')),

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists payment_intents_reference_key
  on payment_intents (reference);

create index if not exists payment_intents_business_id_idx
  on payment_intents (business_id);

-- Reuses the shared `set_updated_at()` trigger function (migration
-- 0001) — not redefined here, same convention every table after
-- `transactions` has followed.
drop trigger if exists payment_intents_set_updated_at on payment_intents;

create trigger payment_intents_set_updated_at
before update on payment_intents
for each row
execute function set_updated_at();
