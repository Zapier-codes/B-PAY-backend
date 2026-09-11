-- Migration 0011: enable Row Level Security on `businesses` and add
-- an explicit, deny-by-default policy.
--
-- Same treatment every other table in this schema gets (migrations
-- 0002, 0004, 0006, 0008): service-role-only access, no
-- `anon`/`authenticated` policy. `businesses` holds account emails
-- and a `status` flag that gates whether a business can transact at
-- all — at least as sensitive as `customers`' PII, and this backend
-- connects to Supabase with a service-role key (bypasses RLS)
-- rather than as an end-user, same reasoning as every prior RLS
-- migration in this schema.
--
-- Same "policy object is a placeholder, not a weaker posture" note
-- migrations 0002/0004 already give: `service_role` already carries
-- BYPASSRLS regardless of this policy, but it's written explicitly
-- so intent is documented in the migration, not left implicit.
--
-- No policy for `anon`/`authenticated` — Task 46's dashboard (which
-- would need a business's own session to read its own row) hasn't
-- been designed yet, and Task 45/c's dashboard-auth question is
-- still open (see migration 0010's own "Not included" note). Deny-
-- all for any non-service-role caller is the safe default until that
-- design lands, not an oversight.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see the DB-Ops Handoff Process
-- section in handover.md.

alter table businesses enable row level security;

create policy businesses_service_role_all
  on businesses
  for all
  to service_role
  using (true)
  with check (true);
