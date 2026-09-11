-- Migration 0005: create the `routing_config` table.
--
-- Task 52/e-2d — resolves the leaf's own open design question (env
-- var vs. config file vs. admin-dashboard toggle) via a fourth option,
-- per the product owner's direct instruction to mirror Stripe's actual
-- answer to this class of problem: Payment Method Configurations are
-- a Dashboard-toggleable, API-backed object with its own ID, not a
-- deploy. This table is that object's B-Pay-backend equivalent — a
-- live, no-redeploy, per-domain-overridable row that
-- `DOMAIN_DEFAULT_PROVIDER` reads at request time (routes.js, Task
-- 52/e-2d's own code change), instead of a hardcoded object literal
-- that needs a deploy to change. See handover.md's Task 52/e-2d entry
-- for the full Stripe-precedent writeup and decision record.
--
-- Scope: this is the platform-level default only — same "platform
-- default, individually overridable" two-tier shape Stripe's own
-- Configurations model uses (a Default Config, overridable per
-- Checkout Session). A per-account/per-domain override tier beyond
-- the two rows this migration seeds is explicitly NOT built here —
-- flagged as a future extension of this same table (an additional
-- scope column), not guessed at in this migration.
--
-- Per Task 52/e-2d's own writeup: "a couple of Supabase-authenticated
-- RPC/SQL calls as your dashboard for now" — there is no admin UI
-- yet (Task 46's dashboard is still decision-record only). Until that
-- exists, the product owner edits this table's rows directly (an
-- `UPDATE routing_config SET default_provider = ... WHERE domain =
-- ...` from the DB-Ops second environment, per handover.md's DB-Ops
-- Handoff Process) to promote a fallback to default, live, with no
-- code deploy — the actual property this task exists to deliver.
--
-- Seeded with the CURRENT hardcoded defaults (routes.js's own
-- `DOMAIN_DEFAULT_PROVIDER` table, Task 52/e-2a), not left empty —
-- this migration must not silently change routing behavior the
-- moment it's applied. `domain` is the primary key: exactly one
-- default per `classifyDomain()` result (`african_rails` |
-- `international`), matching that function's own two possible return
-- values (utils/helpers.js).
--
-- No CHECK constraint on `default_provider` (unlike, say,
-- `transactions.status`) -- deliberately left open so a new provider
-- can be promoted to a domain default via a plain UPDATE, with no
-- migration required to extend an allowed-value list. routes.js's own
-- `getProvider()` is what rejects an unrecognized provider name at
-- request time (a 400/500, not a constraint violation at the DB
-- layer) -- same division of responsibility this repo already uses
-- elsewhere (this table stores the *decision*, application code
-- validates it against what's actually implemented).
--
-- No foreign keys, per the "no placeholder FKs" convention
-- (db/SCHEMA.md) -- nothing else in this repo references
-- `routing_config` rows.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority -- see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

create table if not exists routing_config (
  domain text primary key,
  default_provider text not null,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

-- Reuses the shared `set_updated_at()` trigger function created in
-- migration 0001 -- same convention as `transactions`/`customers`, not
-- a per-table copy of the logic. This is what makes an `UPDATE
-- routing_config SET default_provider = ...` (the actual "promote a
-- fallback to default" action) leave an auditable `updated_at`
-- timestamp behind, with no application code needed to set it.
drop trigger if exists routing_config_set_updated_at on routing_config;

create trigger routing_config_set_updated_at
before update on routing_config
for each row
execute function set_updated_at();

-- Seed: exactly today's live defaults (routes.js's own
-- `DOMAIN_DEFAULT_PROVIDER`), so applying this migration changes
-- nothing about current routing behavior. `on conflict do nothing` --
-- safe to re-run, and deliberately does not overwrite a row the
-- product owner may have already hand-edited between this migration
-- being written and being applied.
insert into routing_config (domain, default_provider) values
  ('african_rails', 'korapay'),
  ('international', 'juicyway')
on conflict (domain) do nothing;
