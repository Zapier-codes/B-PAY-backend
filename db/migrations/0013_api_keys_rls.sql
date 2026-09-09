-- Migration 0013: enable Row Level Security on `api_keys` and add an
-- explicit, deny-by-default policy.
--
-- Same treatment every other table in this schema gets. `api_keys`
-- is the most sensitive table added so far — even though it stores
-- only a Vault *reference*, not a raw secret (migration 0012), the
-- reference itself is enough to decrypt a live, money-adjacent
-- credential via `vault.decrypted_secrets`, so it gets exactly the
-- same service-role-only posture as every other table here, with
-- the same reasoning: this backend connects as `service_role`
-- (bypasses RLS) rather than as an end-user.
--
-- No policy for `anon`/`authenticated` — no dashboard read path onto
-- this table exists yet (Task 46), and even once one does, the
-- correct read path is almost certainly a server-side endpoint that
-- returns `key_prefix` only, never a direct client-side RLS-scoped
-- read of this table — decrypting a Vault secret should stay a
-- deliberate, server-side, audited action, not something a
-- permissive RLS policy hands to any authenticated client session.
-- Flagged for whichever future task designs that read path, not
-- decided here.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see the DB-Ops Handoff Process
-- section in handover.md.

alter table api_keys enable row level security;

create policy api_keys_service_role_all
  on api_keys
  for all
  to service_role
  using (true)
  with check (true);
