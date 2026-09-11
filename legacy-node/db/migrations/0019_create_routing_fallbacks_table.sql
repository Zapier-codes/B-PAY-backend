-- Migration 0019: create the `routing_fallbacks` table.
--
-- Task 63/a — formalizes Task 51's b-1/b-2 prose tables (handover.md:
-- "International rails" / "African rails" domain fallback chains) into
-- a machine-readable, ordered-per-domain list, so a future fallback-
-- attempt implementation (Task 63/b) doesn't have to re-derive
-- provider order from prose. This leaf is data only — no application
-- code (routes.js) reads this table yet; that's Task 63/b's own scope.
--
-- **Deliberately does NOT replace or touch `routing_config`
-- (migration 0005).** `routing_config.default_provider` remains the
-- single source of truth `resolveDomainDefaultProvider()` actually
-- reads at request time today — unchanged by this migration. This
-- table is new and additive: the *full* ordered chain (default +
-- every fallback), for Task 63/b's future use, not a replacement for
-- the existing single-default lookup.
--
-- **Known, flagged tradeoff, not silently resolved:** this creates
-- two tables that both encode "what's the default provider for domain
-- X" — `routing_config.default_provider` and this table's own
-- `priority = 0` row. Today's "promote a fallback to default" workflow
-- (migration 0005's own comment: a live `UPDATE routing_config SET
-- default_provider = ...`) only touches `routing_config` — it does NOT
-- keep this table in sync, so the two CAN drift if one is edited
-- without the other. Not resolved in this leaf, on purpose: reconciling
-- them (e.g. having Task 63/b derive the current default from this
-- table's own `priority = 0` row instead of reading `routing_config`
-- separately, retiring the duplication) is exactly the kind of design
-- call this repo's convention reserves for whoever actually builds the
-- fallback-attempt logic (Task 63/b), not this schema-only leaf.
-- Flagging here so it isn't rediscovered as a surprise later.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see Task 56/b's own table and the DB-Ops
-- Handoff Process section in handover.md.

create table if not exists routing_fallbacks (
  id uuid primary key default gen_random_uuid(),

  -- One of `classifyDomain()`'s two possible return values
  -- (utils/helpers.js) — `'african_rails'` | `'international'`. No
  -- CHECK constraint, same reasoning as `routing_config.default_provider`
  -- (migration 0005): routes.js is what validates this at request
  -- time, not a DB constraint, so extending `classifyDomain()` with a
  -- third domain later needs no migration here.
  domain text not null,

  -- Provider name, same no-CHECK-constraint reasoning as
  -- `routing_config.default_provider` — `getProvider()` (routes.js)
  -- rejects an unrecognized name at request time.
  provider text not null,

  -- 0 = the domain's current default (mirrors `routing_config`'s own
  -- row for the same domain — see the drift caveat above), 1 = first
  -- fallback, 2 = second, etc. Lower attempted first.
  priority integer not null,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  -- Exactly one row per (domain, priority) — no ambiguity about which
  -- provider is "next" for a given domain.
  constraint routing_fallbacks_domain_priority_unique unique (domain, priority),
  -- A provider can't appear twice in the same domain's chain.
  constraint routing_fallbacks_domain_provider_unique unique (domain, provider)
);

-- Reuses the shared `set_updated_at()` trigger function created in
-- migration 0001 — same convention as every other table in this
-- schema, not a per-table copy of the logic.
drop trigger if exists routing_fallbacks_set_updated_at on routing_fallbacks;

create trigger routing_fallbacks_set_updated_at
before update on routing_fallbacks
for each row
execute function set_updated_at();

-- Seed: exactly Task 51's b-1/b-2 tables as they stand today in
-- handover.md (including Task 52/09/09's Flutterwave-row correction),
-- so applying this migration changes no current routing DECISION —
-- it only makes an existing, already-agreed decision queryable.
-- `on conflict do nothing` — safe to re-run.

-- b-1. International rails: Juicyway default; Korapay, Paystack,
-- Flutterwave fallbacks in that order.
insert into routing_fallbacks (domain, provider, priority) values
  ('international', 'juicyway', 0),
  ('international', 'korapay', 1),
  ('international', 'paystack', 2),
  ('international', 'flutterwave', 3)
on conflict (domain, priority) do nothing;

-- b-2. African rails: Korapay default; Paystack, Juicyway, Flutterwave
-- fallbacks in that order.
insert into routing_fallbacks (domain, provider, priority) values
  ('african_rails', 'korapay', 0),
  ('african_rails', 'paystack', 1),
  ('african_rails', 'juicyway', 2),
  ('african_rails', 'flutterwave', 3)
on conflict (domain, priority) do nothing;
