-- Migration 0014: create the `balance_transactions` table.
--
-- Task 61/a (Stripe-Reference Convention, Task 59's own gap analysis —
-- `STRIPE_DISCOVERY.md` §6 named this the single largest structural
-- gap relative to Stripe's own model: `transactions` records
-- individual attempts, but nothing in this schema is an append-only
-- ledger of every event that moves money). This migration is scoped
-- to exactly the table itself, per the "one part per session"
-- mandatory task-splitting rule — Task 61/b (wiring `/pay`, `/payout`,
-- and the VTU routes to actually write here), Task 61/c (a per-
-- business balance view), Task 61/d (reconciliation-job design), and
-- Task 61/e (payout-schedule question) are all still open, not
-- started by this migration.
--
-- Mirrors Stripe's own `BalanceTransaction` object (see
-- `STRIPE_DISCOVERY.md` §6): one row per event that moves a balance
-- (a payment, a payout, a fee, a refund, a manual adjustment) —
-- deliberately a *different* table from `transactions`, not an extra
-- column bolted onto it. `transactions` (migration `0001`) answers
-- "what did this one attempt do"; `balance_transactions` answers
-- "what is the complete, ordered history of everything that has ever
-- moved money," which is a append-only ledger concept `transactions`
-- was never scoped to hold (it's mutable — a `pending` row transitions
-- to `success`/`failed` in place, per migration 0001's own
-- `set_updated_at()` trigger). Keeping the two separate avoids
-- retrofitting ledger semantics onto a table already relied on for a
-- different purpose.
--
-- **Deliberately append-only — the one shared convention this table
-- does NOT follow, flagged explicitly rather than silently deviating:**
-- every other table in this schema gets `updated_at` +
-- `set_updated_at()` (db/SCHEMA.md's own "Shared conventions" list).
-- This table gets neither. A ledger entry that can be edited in place
-- after the fact isn't a ledger — Stripe's own model (and every real
-- accounting ledger) treats a correction as a *new* row (e.g. a
-- refund is its own `type: 'refund'` row referencing the original
-- payment, never a mutation of the original `payment` row's amount).
-- No application code in this repo should ever run an `UPDATE`
-- against this table; only `INSERT`.
--
-- Columns:
--   `business_id` — nullable, `references businesses(id)`. Nullable
--   because not every write path currently knows a business at write
--   time: the VTU routes (Task 58) do carry a `businessId` per
--   request and can populate this, but `/pay`/`/payout` cannot yet —
--   `transactions.business_id` itself doesn't exist (migration 0010's
--   own note: "flagged as separate follow-up scope," still open as of
--   this migration). Populating this column is Task 61/b's job, not
--   this one's; the column exists now so 61/b doesn't need its own
--   migration just to add it.
--
--   `transaction_id` — nullable, `references transactions(id)`. The
--   link back to the attempt-level record, when one exists. Nullable
--   because a manual `adjustment` row (product-owner-initiated
--   correction, not yet built as a feature) may not correspond to any
--   `transactions` row at all.
--
--   `reference` — nullable `text`, deliberately NOT a foreign key to
--   `transactions.reference`. `transactions.reference` is unique but
--   this column's job is plain correlation/lookup convenience (so a
--   row is still searchable by reference even if `transaction_id` is
--   null or the source row is ever deleted) — a hard FK here would
--   force every ledger row to have a matching live `transactions` row,
--   which contradicts the `adjustment` case above.
--
--   `type` — `text` + `CHECK`, per this schema's shared status-column
--   convention. Five values: `'payment'`, `'payout'`, `'fee'`,
--   `'refund'`, `'adjustment'` — the same five Task 59/c's own
--   proposal named. `'fee'`/`'refund'` have no B-Pay write path yet
--   (Task 63/d's unified-refund path is still blocked on its own
--   per-provider discovery pass) — included in the CHECK now anyway
--   so Task 63/d doesn't need a migration of its own just to widen
--   this list later.
--
--   `amount` / `currency` — same types as `transactions.amount`/
--   `transactions.currency` (migration 0001), no new convention.
--
--   `available_on` — nullable `timestamptz`. Mirrors Stripe's own
--   pending-vs-available balance split (`STRIPE_DISCOVERY.md` §6).
--   `null` means "available immediately" (today's honest default —
--   this repo does not currently know, for any of the ten providers,
--   whether there's a real settlement delay before funds are usable,
--   so it does not invent one). A future task can populate this once
--   that's actually confirmed per provider — not guessed at here.
--
-- Not included, flagged rather than silently omitted:
--   - No `status` column. Stripe's own `BalanceTransaction` doesn't
--     carry one either — a ledger row is a statement of fact about
--     something that already happened, not a thing with its own
--     lifecycle the way a `transactions` row (pending → success/
--     failed) has. A reversal is a new row, per this table's own
--     append-only design above, not a status change on an old one.
--   - No `RLS` in this migration — paired into `0015`, same pattern
--     every prior create-table migration in this schema uses (0001/
--     0002, 0003/0004, 0005/0006, 0007/0008, 0010/0011, 0012/0013).
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see Task 56/b's own table and the
-- DB-Ops Handoff Process section in handover.md.

create table if not exists balance_transactions (
  id uuid primary key default gen_random_uuid(),

  business_id uuid references businesses(id),
  transaction_id uuid references transactions(id),
  reference text,

  provider text not null,

  type text not null
    check (type in ('payment', 'payout', 'fee', 'refund', 'adjustment')),

  amount numeric not null,
  currency text not null,

  available_on timestamptz,

  created_at timestamptz not null default now()
);

-- Lookup patterns this table exists to serve: "everything for this
-- business" (Task 61/c's per-business balance view), "everything for
-- this attempt" (tracing a `transactions` row forward to its ledger
-- entries), and "everything by reference" (correlation when
-- `transaction_id` is null). No index on `created_at` alone yet —
-- every real query so far is scoped by one of the three columns
-- below first; a time-range index can be added later if a
-- reconciliation job (Task 61/d) needs one once that job's actual
-- query shape is known, rather than guessed at now.
create index if not exists balance_transactions_business_id_idx
  on balance_transactions (business_id);

create index if not exists balance_transactions_transaction_id_idx
  on balance_transactions (transaction_id);

create index if not exists balance_transactions_reference_idx
  on balance_transactions (reference);
