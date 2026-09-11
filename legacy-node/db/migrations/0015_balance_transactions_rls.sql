-- Migration 0015: enable Row Level Security on `balance_transactions`.
--
-- Same pattern and same reasoning as every prior create-table
-- migration's RLS pairing (0002, 0004, 0006, 0008, 0011, 0013): this
-- backend talks to Supabase with a service-role key (bypasses RLS by
-- default), so a real per-business policy only matters once Task 46's
-- dashboard reads this table directly with a non-service-role
-- connection — not yet the case. RLS is still enabled now, with one
-- explicit `service_role` policy, so the intent is documented in a
-- migration rather than left implicit, per every prior table's own
-- convention. No policy for `anon`/`authenticated` — under Postgres
-- RLS, no matching policy means no access, which is the correct
-- default until a dashboard-scoped policy actually has a caller to
-- serve.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see Task 56/b's own table and the
-- DB-Ops Handoff Process section in handover.md.

alter table balance_transactions enable row level security;

create policy balance_transactions_service_role_all
  on balance_transactions
  for all
  to service_role
  using (true)
  with check (true);
