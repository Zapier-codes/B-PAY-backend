-- Migration 0001: create the `transactions` table.
--
-- Per Task 56/b's incremental, on-the-go schema convention: this is
-- the FIRST migration, so it also locks in the shared conventions
-- every later migration follows (id type, timestamp columns, status-
-- column style). See db/SCHEMA.md for the current, running summary
-- of the whole schema — update that file in the same session as any
-- migration that changes it.
--
-- Scope, per Task 56/d-1: only what's needed to resolve
-- GET /payout/verify's currency gap (Task 52/e-2b-ii) and to give
-- POST /pay + POST /payout somewhere to write to (Task 56/d-3).
-- No `business_id` or other foreign key yet — per Task 56/b's
-- "no placeholder FKs" rule, those get added in a later migration
-- once a `businesses` table actually exists.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see Task 56/b's own table and the
-- DB-Ops Handoff Process section in handover.md.

-- `gen_random_uuid()` needs pgcrypto. Supabase projects normally have
-- it available already, but this is declared explicitly rather than
-- assumed, so this migration doesn't silently depend on a project's
-- own out-of-band setup.
create extension if not exists pgcrypto;

create table if not exists transactions (
  id uuid primary key default gen_random_uuid(),

  -- Unique, indexed (via the index below) — this is what
  -- GET /payout/verify looks up by (Task 56/d-4).
  reference text not null,

  -- Which route wrote this row. Both POST /pay and POST /payout
  -- write here (Task 56/d-3).
  type text not null check (type in ('payment', 'payout')),

  provider text not null,
  currency text not null,
  amount numeric not null,

  -- Shared status-column convention (see db/SCHEMA.md): `text` + a
  -- CHECK constraint, not a Postgres `enum` — a later migration can
  -- extend the allowed list without an `ALTER TYPE`.
  status text not null default 'pending'
    check (status in ('pending', 'success', 'failed')),

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists transactions_reference_key
  on transactions (reference);

-- Shared `updated_at` convention: Postgres has no built-in
-- `ON UPDATE` clause, so every table that wants an auto-updating
-- `updated_at` uses this one trigger function, not a copy of the
-- logic per table.
create or replace function set_updated_at()
returns trigger as $$
begin
  new.updated_at = now();
  return new;
end;
$$ language plpgsql;

drop trigger if exists transactions_set_updated_at on transactions;

create trigger transactions_set_updated_at
before update on transactions
for each row
execute function set_updated_at();
