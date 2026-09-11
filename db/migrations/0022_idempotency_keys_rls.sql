-- Migration 0022: enable Row Level Security on `idempotency_keys` and
-- add an explicit, deny-by-default policy.
--
-- Same treatment as every other table in this schema — service-role-
-- only access, no `anon`/`authenticated` policy. This table caches
-- full response bodies keyed by a caller-supplied string; an open
-- `anon`/`authenticated` read policy would let one caller potentially
-- read another business's cached response by guessing/reusing an
-- `Idempotency-Key` value, and an open write policy would let a
-- caller poison the cache with a fabricated response for a key it
-- doesn't own. This backend connects to Supabase with a service-role
-- key (which bypasses RLS), same posture as every other table in this
-- schema — see utils/supabase.js's own service-role-key comment.
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

alter table idempotency_keys enable row level security;

create policy idempotency_keys_service_role_all
  on idempotency_keys
  for all
  to service_role
  using (true)
  with check (true);
