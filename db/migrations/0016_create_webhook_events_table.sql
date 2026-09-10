-- Migration 0016: create the `webhook_events` table.
--
-- Task 60/a (closes `STRIPE_DISCOVERY.md` §3's gap — Stripe's own
-- "every delivery attempt is a queryable record" posture). This
-- migration is scoped to exactly the table itself, per the "one part
-- per session" mandatory task-splitting rule — Task 60/b (dedup check
-- in `webhookGateway.js`), Task 60/c (per-provider discovery of the
-- real event-id field for Paystack/Korapay/JuicyWay/TelcosOpik), Task
-- 60/d (manual replay route), and Task 60/e (continuous-failure
-- alerting, blocked on a product-owner decision) are all still open,
-- not started by this migration. Same division of labor Task 61/a set
-- for `balance_transactions`: storage only, nothing wired yet.
--
-- Columns:
--   `provider` — `text not null`, same type/nullability as
--   `balance_transactions.provider` (migration 0014). No `CHECK`
--   constraint against a fixed provider list, matching that column's
--   own precedent — this schema does not currently enumerate the ten
--   providers as a Postgres-level constraint anywhere.
--
--   `provider_event_id` — nullable `text`. This is B-Pay's own column
--   name, not a claim about what any provider calls the field in its
--   own payload — Task 60/c's own discovery pass is what maps each
--   provider's real field (e.g. Paystack's `data.id` vs whatever
--   Korapay/JuicyWay/TelcosOpik use) onto this column, and that
--   mapping is deliberately not guessed at here. Nullable because a
--   row must still be insertable (and the raw `payload` preserved)
--   even before that mapping exists for a given provider, or if a
--   provider's payload is ever missing the field outright — Task
--   60/b's dedup lookup only works once this is reliably populated,
--   flagged as that task's own dependency, not assumed solved by this
--   migration.
--
--   `payload` — `jsonb not null`. The full, raw webhook body, so
--   nothing is lost if a later session needs a field this migration
--   didn't anticipate, and so Task 60/d's replay mechanism has the
--   exact original payload to re-run a handler against rather than a
--   reconstruction.
--
--   `signature_valid` — `boolean not null`. Recorded regardless of
--   outcome (including `false`) — same "queryable record of every
--   delivery attempt" posture as the rest of this table, not just the
--   attempts that passed verification.
--
--   `status` — `text` + `CHECK`, per this schema's shared status-
--   column convention (`db/SCHEMA.md`'s "Shared conventions" list).
--   Three values: `'received'`, `'processed'`, `'failed'` — the same
--   three Task 60's own top-level write-up named. This is a genuine
--   lifecycle column (unlike `balance_transactions.type`, which is
--   deliberately status-free per migration 0014's own note) — a row
--   is expected to transition `received` → `processed`/`failed` in
--   place as `webhookGateway.js` actually runs the handler, which is
--   why this table (unlike `balance_transactions`) keeps the shared
--   `updated_at` + `set_updated_at()` convention below.
--
--   `received_at` — `timestamptz not null default now()`. When the
--   row was first written, before any handler has run.
--
--   `processed_at` — nullable `timestamptz`. Set once `status` moves
--   to `processed`/`failed`; null while still `received`. Left as a
--   plain nullable column rather than a trigger-maintained one, since
--   "processed" here is an application-level outcome
--   (`webhookGateway.js` deciding a handler finished), not a raw
--   row-level `UPDATE` the shared trigger function could infer on its
--   own.
--
-- Not included, flagged rather than silently omitted:
--   - No FK from this table to `transactions`/`balance_transactions`.
--     A webhook delivery doesn't always correspond to exactly one
--     `transactions` row at the time it's received (that mapping is
--     each provider handler's own job, inside `payload`), so this
--     table doesn't assume one — same reasoning
--     `balance_transactions.reference` (migration 0014) already used
--     for staying a plain column instead of a hard FK.
--   - No `RLS` in this migration — paired into `0017`, same pattern
--     every prior create-table migration in this schema uses (0001/
--     0002, 0003/0004, 0005/0006, 0007/0008, 0010/0011, 0012/0013,
--     0014/0015).
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see Task 56/b's own table and the
-- DB-Ops Handoff Process section in handover.md.

create table if not exists webhook_events (
  id uuid primary key default gen_random_uuid(),

  provider text not null,
  provider_event_id text,

  payload jsonb not null,
  signature_valid boolean not null,

  status text not null default 'received'
    check (status in ('received', 'processed', 'failed')),

  received_at timestamptz not null default now(),
  processed_at timestamptz,

  updated_at timestamptz not null default now()
);

create trigger set_webhook_events_updated_at
  before update on webhook_events
  for each row
  execute function set_updated_at();

-- Lookup patterns this table exists to serve: Task 60/b's dedup check
-- (`(provider, provider_event_id)` before running any handler side
-- effect) and "everything from this provider" for Task 60/d's manual
-- replay route. No index on `status`/`received_at` alone yet — Task
-- 60/e's alerting leaf is still blocked on a product-owner decision,
-- so its real query shape isn't known; an index for it can be added
-- once that leaf is actually picked up, not guessed at now.
create index if not exists webhook_events_provider_event_idx
  on webhook_events (provider, provider_event_id);
