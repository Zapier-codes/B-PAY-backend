-- Migration 0024: enable Row Level Security on `payment_intents` and
-- add an explicit, deny-by-default policy.
--
-- Same treatment as every other table in this schema — service-role-
-- only access, no `anon`/`authenticated` policy. This backend
-- connects to Supabase with a service-role key (which bypasses RLS),
-- same posture as every other table here — see utils/supabase.js's
-- own service-role-key comment.
--
-- No policy for `anon` or `authenticated`. Under Postgres RLS,
-- enabling RLS with no matching policy for a role denies that role
-- all access by default — the safe default here, same as every other
-- table in this schema.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

alter table payment_intents enable row level security;

create policy payment_intents_service_role_all
  on payment_intents
  for all
  to service_role
  using (true)
  with check (true);
