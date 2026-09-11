-- Migration 0002: enable Row Level Security on `transactions` and add
-- an explicit placeholder policy.
--
-- Per Task 56/d-5: "a placeholder/permissive policy is acceptable for
-- now, since this backend talks to Supabase with a service-role key
-- (which bypasses RLS) rather than as an end-user" — but a policy
-- must actually be written, not silently skipped (Task 53/54's own
-- already-flagged "no RLS design exists yet" gap). Real per-business
-- RLS only matters once a dashboard with business-level logins
-- (Task 46) actually reads from this table directly — that is a
-- later migration, not this one.
--
-- Design choice, flagged plainly rather than papered over: "placeholder/
-- permissive" is read here as "the POLICY object itself may be a
-- simple placeholder", not as "leave every role wide open". This
-- migration enables RLS and grants exactly one explicit policy, scoped
-- only to `service_role` — the role this backend actually connects as
-- (see utils/supabase.js's own service-role-key comment). That
-- `service_role` grant is redundant today (Supabase's `service_role`
-- already carries BYPASSRLS and ignores RLS entirely regardless of any
-- policy), but it's written explicitly anyway so this table's RLS
-- intent is documented in a migration file rather than left implicit,
-- and so behavior doesn't silently change if that role's BYPASSRLS
-- attribute is ever revoked at the project level.
--
-- No policy is created for `anon` or `authenticated`. Under Postgres
-- RLS, enabling RLS with no matching policy for a role denies that
-- role all access by default — so this is the safe default (deny-all
-- for any non-service-role caller) rather than an oversight. There is
-- no end-user-facing read path into this table yet (Task 46's
-- dashboard hasn't landed), so there is nothing for a permissive
-- `anon`/`authenticated` policy to usefully enable today, and no
-- reason to open one before it's needed.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

alter table transactions enable row level security;

create policy transactions_service_role_all
  on transactions
  for all
  to service_role
  using (true)
  with check (true);
