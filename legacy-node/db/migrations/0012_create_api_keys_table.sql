-- Migration 0012: create the `api_keys` table.
--
-- Separate table rather than columns on `businesses`, per Task 45/b's
-- own reasoning: "so a business can hold multiple keys (e.g. live +
-- test) and so a compromised key can be revoked without touching the
-- business row itself." First concrete use: storing each business's
-- own `telcos.opik.net` credential (Task 58/c), one row per
-- (business, provider) pair — designed generically enough that a
-- future provider needing the same per-business-credential shape
-- reuses this table rather than getting its own.
--
-- Encryption at rest (Task 58/c-2, Stripe-mirrored envelope
-- encryption): this table does NOT store the raw secret value.
-- `vault_secret_id` is a reference into Supabase Vault
-- (`vault.secrets.id`, pgsodium-backed) — the actual secret is
-- inserted separately via `select vault.create_secret(<raw key>,
-- <name>, <description>)`, which returns the uuid stored here.
-- Reading the real value back requires an explicit
-- `select decrypted_secret from vault.decrypted_secrets where id =
-- <vault_secret_id>` — Vault is a Supabase-managed extension schema,
-- not a table this migration creates, so no FK constraint is added
-- against it here (consistent with this schema's "no placeholder FK
-- across a boundary this migration doesn't own" caution); the
-- reference is enforced at the application layer
-- (`getProviderKey('telcosopik', businessId)`, Task 58/c) instead.
-- **No raw key value is ever written to this table or logged** —
-- same "never log a secret" posture this repo already holds for
-- every other provider key (env-var-based ones included).
--
-- Duplicate-registration guard (Task 58/c-3, Stripe-mirrored
-- check-then-create): `unique (business_id, provider)` is the DB-
-- level half of that guard — it's the actual enforcement point if
-- `telcosopik.js`'s own application-layer "check before calling
-- POST /auth/register" logic ever loses a race (e.g. two concurrent
-- first-VTU-activation requests for the same business). An insert
-- that violates this constraint should be caught and treated the
-- same way Task 58/c-3 describes a live 409 from `telcos.opik.net`
-- itself: "someone else's race won," fall back to reading the
-- already-inserted row rather than surfacing a hard failure.
--
-- `key_prefix` — a short, non-secret display fragment (e.g. the
-- `sk_live_` prefix plus a few trailing characters, Stripe-dashboard
-- style: "sk_live_...a1b2"), so a business/admin UI can show which
-- key a row corresponds to without ever decrypting the Vault secret
-- for display purposes. Nullable — not every provider's key format
-- necessarily has a meaningful prefix to show.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase
-- project on its own authority — see the DB-Ops Handoff Process
-- section in handover.md.

create table if not exists api_keys (
  id uuid primary key default gen_random_uuid(),

  business_id uuid not null references businesses(id) on delete cascade,
  provider text not null,
  vault_secret_id uuid not null,
  key_prefix text,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  unique (business_id, provider)
);

create index if not exists api_keys_business_id_idx on api_keys (business_id);

-- Reuses the shared `set_updated_at()` trigger function created in
-- migration 0001 — same convention as every other table in this
-- schema.
drop trigger if exists api_keys_set_updated_at on api_keys;

create trigger api_keys_set_updated_at
before update on api_keys
for each row
execute function set_updated_at();
