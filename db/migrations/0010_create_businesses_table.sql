-- Migration 0010: create the `businesses` table.
--
-- Pulled forward from Task 45/b (Supabase-backed dashboard, decided
-- but not yet built) as Task 58's own step 0/3 — Task 58/c's
-- 2026-09-09 revision makes this a hard dependency: VTU account
-- provisioning has nowhere to store a per-business
-- `telcos.opik.net` credential without this table existing first.
-- This migration is scoped to exactly what Task 58 needs to proceed;
-- it does not attempt Task 45/b's or Task 46's full dashboard scope
-- (see "Not included" below).
--
-- This is also the table migration 0001's own header comment
-- flagged as future work ("No `business_id` or other foreign key
-- yet ... those get added in a later migration once a `businesses`
-- table actually exists") — that FK is deliberately NOT added in
-- this migration either, per the "only the current atomic leaf"
-- task-splitting rule. Wiring `transactions.business_id` (and
-- `customers.business_id`) is flagged as a natural follow-up task,
-- not done here, so this migration stays scoped to what Task 58
-- itself needs.
--
-- Columns:
--   `status` — `'active'` | `'suspended'`, default `'active'`.
--   Mirrors Task 45/b's own proposed shape ("a `status` column that
--   every authenticated request checks before doing anything else —
--   fail closed"). No CHECK constraint on the two values yet, same
--   reasoning migration 0003 gave for `customers.customer_type`:
--   left as unconstrained `text` rather than a hand-guessed enum,
--   since Task 46's full dashboard (which actually needs a richer
--   status model — e.g. a pending/review state) hasn't been designed
--   yet and a CHECK written now could need immediate widening.
--
--   `email` — unique. This is the business's B-Pay account identity,
--   not their `telcos.opik.net` credential (that lives in `api_keys`,
--   migration 0012) — kept as two separate concerns, same "don't
--   conflate the vault entry with the account row" reasoning Task
--   45/c's own dashboard-auth note already raised for API secret keys
--   vs. dashboard login.
--
-- Not included, flagged rather than silently omitted:
--   - No password/auth-credential column. Task 45/c's own open item
--     ("a business's dashboard login is not the same credential as
--     their API secret key") is still genuinely unresolved — this
--     table intentionally has nowhere to put a dashboard password
--     yet, so a future migration doesn't have to un-do a wrong guess
--     made here.
--   - No `business_id` FK added to `transactions`/`customers` in this
--     migration (see above) — flagged as separate follow-up scope.
--   - No admin/role column (Task 45/c's admin-view requirement) —
--     out of scope for what Task 58 itself needs.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see the DB-Ops Handoff Process
-- section in handover.md.

create table if not exists businesses (
  id uuid primary key default gen_random_uuid(),

  email text not null unique,
  company_name text,
  status text not null default 'active',

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

-- Reuses the shared `set_updated_at()` trigger function created in
-- migration 0001 — same convention as `transactions`/`customers`,
-- not a per-table copy of the logic.
drop trigger if exists businesses_set_updated_at on businesses;

create trigger businesses_set_updated_at
before update on businesses
for each row
execute function set_updated_at();
