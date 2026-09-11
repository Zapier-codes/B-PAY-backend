-- Migration 0008: enable Row Level Security on `capabilities` and add
-- an explicit, deny-by-default policy.
--
-- Same treatment migrations 0002 (`transactions`), 0004 (`customers`),
-- and 0006 (`routing_config`) already established -- service-role-only
-- access, no `anon`/`authenticated` policy. Same convention, new
-- table. This backend connects to Supabase with a service-role key
-- (which bypasses RLS), same posture as every other table in this
-- schema -- see utils/supabase.js's own service-role-key comment.
--
-- Design choice, same note as migrations 0002/0004/0006:
-- "placeholder/permissive" is read as "the POLICY object itself may be
-- a simple placeholder", not as "leave every role wide open". This
-- migration enables RLS and grants exactly one explicit policy, scoped
-- only to `service_role` -- redundant with that role's own BYPASSRLS
-- today, written explicitly anyway so this table's RLS intent is
-- documented in a migration file rather than left implicit.
--
-- No policy for `anon` or `authenticated`. Under Postgres RLS,
-- enabling RLS with no matching policy for a role denies that role
-- all access by default -- the safe default here, same as every other
-- table in this schema. If Task 46's admin dashboard ever gets a real
-- end-user-facing edit path for this table (e.g. flipping a
-- capability live from a UI instead of direct SQL), that's a new,
-- carefully-scoped policy added in its own future migration -- not
-- assumed or half-built here.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority -- see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

alter table capabilities enable row level security;

create policy capabilities_service_role_all
  on capabilities
  for all
  to service_role
  using (true)
  with check (true);
