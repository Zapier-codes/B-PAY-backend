-- Migration 0026: enable Row Level Security on `payment_attempts` and
-- add an explicit, deny-by-default policy.
--
-- Same treatment as every other table in this schema — service-role-
-- only access, no `anon`/`authenticated` policy. No policy for `anon`
-- or `authenticated`: under Postgres RLS, enabling RLS with no
-- matching policy for a role denies that role all access by default —
-- the safe default here, same as every other table in this schema.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

alter table payment_attempts enable row level security;

create policy payment_attempts_service_role_all
  on payment_attempts
  for all
  to service_role
  using (true)
  with check (true);
