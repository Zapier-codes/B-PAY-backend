-- Migration 0007: create the `capabilities` table.
--
-- Task 52/e-2e — resolves this leaf's "not started, but not blocked
-- on a decision either" status via the Stripe-precedent the product
-- owner directed this task to mirror: the Capabilities API on
-- Stripe's own Account object. Each Stripe capability (card_payments,
-- transfers, treasury, ...) is tracked as its OWN independent entity
-- with its own `status` and `requirements` — an account can have some
-- capabilities active and others inactive at the same time, and a
-- capability can be requested/tracked long before every requirement
-- behind it is actually satisfied. See handover.md's Task 52/e-2e
-- entry for the full precedent writeup and decision record.
--
-- The gap this resolves: e-2e read as "not buildable yet" because it
-- was implicitly waiting for a real `/kyc`-style route (Task 53/54)
-- to exist before any routing/status logic could be written. Stripe's
-- own shape says that's backwards -- the capability abstraction (an
-- entity with an id and a status) doesn't need its underlying
-- implementation finished to exist. This table is that abstraction:
-- a small, explicit list of every capability this platform's own
-- domain model names (Task 51's collection/payout/banks, Task 53/54's
-- still-decision-record kyc/card_issuance/gift_cards/vtu), each with a
-- `status` reflecting what's REALLY true in this codebase today, not
-- aspirational. A capability-mix flow (Task 55/b) can check a
-- capability's status before routing to it and degrade cleanly (a 501,
-- same shape `GET /payout/verify` already uses for
-- `verifyPayout is not a function`) instead of assuming it exists.
--
-- Seeded (this migration) with today's REAL status per capability --
-- not a guess: `collection`/`payout`/`banks` are `active` because
-- POST /pay, POST /payout, and GET /banks are live, working routes in
-- this repo today (Task 52/e-2a/b-i/c). `kyc`, `card_issuance`,
-- `gift_cards`, and `vtu` are `not_implemented` because Tasks 53, 54,
-- 48, and 44 are each still decision-record only -- no actual route
-- for any of them exists in this codebase yet. This migration does
-- not change what any of those tasks have or haven't built; it only
-- gives their current state a queryable row.
--
-- `status` uses the same `text` + `CHECK` convention as
-- `transactions.status` (migration 0001) -- easier for a later
-- migration to extend the allowed list than a Postgres enum's
-- `ALTER TYPE`. Three values, not Stripe's own richer
-- unrequested/pending/active/disabled set -- this platform doesn't yet
-- have Stripe's own requirements-collection workflow (no per-
-- capability onboarding form, no `requirements` hash to track), so a
-- fourth/fifth status value would be unused ceremony today. `pending`
-- is included even though nothing seeds as `pending` yet, so a future
-- capability that's built but not yet verified/live (mirroring
-- Stripe's own capability lifecycle) has somewhere to sit without
-- another migration.
--
-- No foreign keys, per the "no placeholder FKs" convention
-- (db/SCHEMA.md) -- nothing else in this repo references
-- `capabilities` rows.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority -- see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

create table if not exists capabilities (
  capability text primary key,
  status text not null default 'not_implemented'
    check (status in ('active', 'pending', 'not_implemented')),

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

-- Reuses the shared `set_updated_at()` trigger function created in
-- migration 0001 -- same convention as every other table in this
-- schema. This is what makes flipping a capability from
-- `not_implemented` to `active` (the moment Task 53/54's real route
-- lands) leave an auditable `updated_at` timestamp, same as
-- `routing_config`'s own "promote a fallback to default" edit
-- (migration 0005).
drop trigger if exists capabilities_set_updated_at on capabilities;

create trigger capabilities_set_updated_at
before update on capabilities
for each row
execute function set_updated_at();

-- Seed: today's real status per capability (see this migration's own
-- comment above for the reasoning behind each value). `on conflict do
-- nothing` -- safe to re-run, and deliberately does not overwrite a
-- row the product owner may have already hand-edited (e.g. once a
-- future task actually ships kyc and flips it to `active`) between
-- this migration being written and being applied.
insert into capabilities (capability, status) values
  ('collection', 'active'),
  ('payout', 'active'),
  ('banks', 'active'),
  ('kyc', 'not_implemented'),
  ('card_issuance', 'not_implemented'),
  ('gift_cards', 'not_implemented'),
  ('vtu', 'not_implemented')
on conflict (capability) do nothing;
