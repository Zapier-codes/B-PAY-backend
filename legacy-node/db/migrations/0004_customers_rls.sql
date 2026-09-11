-- Migration 0004: enable Row Level Security on `customers` and add an
-- explicit, deny-by-default policy.
--
-- Per Task 57/c: same treatment Task 56/d-5 already established for
-- `transactions` (migration 0002) — service-role-only access, no
-- `anon`/`authenticated` policy. Same convention, new table. This
-- table's own case for that treatment is at least as strong as
-- `transactions`': `customers` holds names, phone numbers, and
-- billing addresses (real PII across ten payment providers), not just
-- transaction metadata, and — same as `transactions` — this backend
-- connects to Supabase with a service-role key (which bypasses RLS)
-- rather than as an end-user, so a placeholder/permissive-object,
-- deny-by-default-in-practice policy is appropriate for the same
-- reason it was for migration 0002, not a weaker one.
--
-- Design choice, flagged plainly, same as migration 0002's own note:
-- "placeholder/permissive" is read as "the POLICY object itself may be
-- a simple placeholder", not as "leave every role wide open". This
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
-- no end-user-facing read path into this table yet (Task 57/d/e, which
-- actually read/write it, haven't landed, and Task 46's dashboard
-- hasn't either) — there is nothing for a permissive
-- `anon`/`authenticated` policy to usefully enable today, and every
-- reason not to open one early given what this table stores.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

alter table customers enable row level security;

create policy customers_service_role_all
  on customers
  for all
  to service_role
  using (true)
  with check (true);
