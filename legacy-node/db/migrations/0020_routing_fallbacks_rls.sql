-- Migration 0020: enable Row Level Security on `routing_fallbacks`
-- and add an explicit, deny-by-default policy.
--
-- Same treatment as `routing_config` (migration 0006) — service-role-
-- only access, no `anon`/`authenticated` policy. This table isn't
-- read by application code yet (Task 63/a is data-only; Task 63/b is
-- the future read path), but per this schema's own standing
-- convention every table gets RLS enabled in the same session it's
-- created, not deferred until something reads it — an open
-- `anon`/`authenticated` write policy here would let a caller reorder
-- or redirect a domain's fallback chain, same real-money concern
-- migration 0006's own comment already raised for `routing_config`.
--
-- This backend connects to Supabase with a service-role key (which
-- bypasses RLS), same posture as every other table in this schema —
-- see utils/supabase.js's own service-role-key comment.
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

alter table routing_fallbacks enable row level security;

create policy routing_fallbacks_service_role_all
  on routing_fallbacks
  for all
  to service_role
  using (true)
  with check (true);
