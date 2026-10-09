# Handover — Zapier-codes/B-Pay-backend infrastructure

This file documents an infra investigation session (2026-09-20) into why CI was
failing and what's actually needed to get this repo running on its own Render
service. Read this fully before touching CI/Docker/Render config again — it
will save you from re-doing the same discovery.

---

## Product Vision — full picture (read this first)

Added 2026-09-22, consolidating scattered context from this and prior
sessions into one place. Two categories below: **live today** (verified
against the actual Render/GitHub/Supabase state) and **documented, not yet
built** (a real plan, spot-checked against this codebase so it's grounded
in what actually exists here — but zero lines of it are written yet).
Don't assume anything in the second category is implemented just because
it's written down.

### The foundation (live today)

Two independently-forked, independently-running instances of the same
Hyperswitch-based payment orchestration engine. `Phoenix-Boss/B-PAY-backend`
(upstream) still runs its own Node.js service; this fork,
`Zapier-codes/B-Pay-backend`, replaced that with the full Rust/Hyperswitch
codebase and runs as its own separate product on its own infrastructure —
nothing shared with upstream except git history.

**Deployment pipeline** (same pattern for every service): GitHub Actions
builds a Docker image with `type=gha`/`mode=max` layer caching, pushes to
GHCR, and Render pulls that prebuilt image — **image-backed, not
Render-building-from-source** (see the correction under "What's actually
needed before creating the new Render service," point 5, below). A deploy
hook fires on every push to `main`. Two services run this way today:
`b-pay-backend-new` (this API) and `control-center-new` (Hyperswitch's
open-source merchant dashboard), wired to the backend via its API URL —
see "`control-center` deployment specifics" below for the exact config.
Both confirmed live, passing `/health`, as of the 2026-09-22 session update
further down this file.

**Database:** a single Supabase Postgres instance serves all four internal
data roles (master/replica/accounts/global) and doubles as the KV/cache
backend via the app's built-in Postgres-KV mode (`KV_BACKEND=postgres`,
see point 3 under "What's actually needed..." below) — no separate Redis
server.

### Removing AWS entirely (documented, not yet built)

Each AWS-backed subsystem has a specific, code-checked replacement —
"code-checked" meaning spot-verified to exist as an extension point in
this codebase, not that any of it is implemented:

- **Novu** for all email. `crates/external_services/src/email.rs` defines
  a real `EmailClient` trait with `ses.rs`/`smtp.rs`/`no_email.rs`
  implementations already; a Novu backend would be a fourth
  implementation of that same trait. Nothing Novu-specific exists in the
  codebase yet — confirmed via a repo-wide grep, zero matches.
- **Cloudflare R2** for file storage.
  `crates/external_services/src/file_storage/aws_s3.rs` is a real,
  existing S3 client. Since R2 is S3-API-compatible, this is extending
  that client with a custom endpoint, not a rewrite — and no egress fees.
- **ClickHouse Cloud (free tier)** for analytics. `crates/analytics/src/`
  already has a real `clickhouse.rs` plus per-domain modules
  (`refunds/core.rs`, `disputes/core.rs`, etc.) built specifically for
  ClickHouse — this is provisioning a real instance, not new code.

### The front door: a Stripe-caliber landing page (documented, not yet built)

An original-content, original-design marketing site matching that tier of
polish — animated hero, scroll-triggered sections, an interactive API
showcase — deployed as a third sibling service using the exact same
CI/CD pattern as the other two. Direct signup, not gated behind a
"contact us" form: "Get Started" lands straight on Control Center's own
registration. **See "NEW TASK — Industry landing page" near the end of
this file for the full 7-subtask breakdown** — that section is the
authoritative, detailed version of this item; this paragraph is only the
summary pointer.

### Onboarding: gamified, Google-first, progressive (documented, not yet built)

Users sign up with **Google**, via the app's existing generic OIDC
framework configured for Google specifically — a real OIDC subsystem
exists (`crates/api_models/src/oidc.rs`,
`crates/router/src/types/domain/user/oidc.rs`,
`crates/router/src/consts/oidc.rs`), but it is not currently wired to a
Google provider or to any gamified-checklist UI. No business paperwork
required to start. The account would land in a visible, gamified state — a
progress checklist in Control Center ("Activate your account — 2/5
complete") rather than a hard wall, nudged along by Novu emails (see AWS
removal, above) when someone stalls partway. None of this checklist/nudge
logic exists yet.

### Trust and risk: the tiered KYC/KYB model (documented, not yet built — no code exists for this at all)

Grounded in how Paystack and other real processors actually operate
(checked against their own docs and a real Financial Ombudsman ruling
validating the pattern), not yet checked against anything in *this*
codebase because there is nothing here to check — a repo-wide search for
suspension/threshold logic returns zero matches, and existing
`kyc`/`kyb` string matches are narrow, connector-specific fields
(`facilitapay`, `nomupay`) unrelated to a platform-wide model. The
proposed design: any user, any country, verified or not, can send and
receive live payments immediately — full normal experience, no visible
restriction. A threshold (amount, over a period) applies silently
underneath, visible only to admins, never exposed to the merchant.
Crossing it triggers automatic suspension plus a Novu email explaining
that verification is now required. Submitting KYC (individual identity)
or full KYB (business registration, beneficial ownership) lifts the
suspension and raises or removes the threshold, mirroring Paystack's own
Starter→Registered progression. Payouts to a *new* bank destination would
get their own separate verification checkpoint, independent of the
collection threshold, matching how Paystack gates money actually leaving
the platform. Whoever picks this up next needs to design and build all of
it from scratch — treat this paragraph as a spec, not a status report.
**One specific, more generous special case within this model has its own
dedicated task below: "NEW TASK — Unregistered business: 1-year full-
access grace period."** Read that section before implementing the general
threshold model, since it changes what "the threshold" means for this
particular segment of users.

### Feature and country gating: driven by Superposition (partially live — Superposition itself is integrated; the gating rules described here are not)

Superposition — the dynamic config service already integrated into the
app (see PRIORITY 2 above: it's a required, non-optional boot dependency,
not feature-flagged) — is real and running. What's proposed, not yet
built: routing every tier/country/verification-level distinction (which
payment methods, currencies, and dashboard features a given merchant
sees) through Superposition, keyed off `merchant_business_country`
(confirmed real field, see `crates/connector_configs/toml/development.toml`)
and a verification tier that does not yet exist (see the KYC/KYB section
above) — rather than scattering that logic across the codebase.

---

## PRIORITY 1 — Apply this patch first

**File:** `0001-priority1-render-fix.patch` (repo root, alongside this file)

This is a single `git format-patch` file containing one commit that touches
three files (`.github/workflows/docker-publish.yml`, `Dockerfile`, and this
`HANDOVER.md` itself). Apply with `git am` (not `git apply`) so it lands as
one real commit with its original message/author, not just a working-tree
diff:

```bash
git am 0001-priority1-render-fix.patch
# or, if that fails due to line-ending/context drift:
git am --3way 0001-priority1-render-fix.patch
```

The commit contains two independent fixes, bundled because they were found
in the same session:

1. **`.github/workflows/docker-publish.yml`** — the "Trigger Render deploy
   hook" step used to `exit 1` (failing the whole CI job) whenever
   `RENDER_DEPLOY_HOOK_URL` wasn't set as a repo secret. Fixed so the step is
   *skipped* (not failed) when the secret is absent, via a step-level `if:`
   condition instead of a manual shell check. Once `RENDER_DEPLOY_HOOK_URL`
   is added as a secret (see below), the step activates automatically.

2. **`Dockerfile`** — the runtime image only bakes in
   `config/payment_required_fields_v2.toml`. Every other config file
   (`sandbox.toml`, `production.toml`, etc.) is expected to arrive via a
   `docker-compose` volume mount of `./config`. A **standalone** container
   (e.g. a single Render web service with no compose orchestration) has no
   such mount. Because `settings.rs` loads the config file with
   `.required(false)`, a missing file does **not** crash the app — it just
   silently boots with almost nothing configured. The patch bakes
   `config/docker_compose.toml` (the project's own known-working default
   set) into the image at the exact path `RUN_ENV` resolves to, so a
   standalone deploy always has a real base config. Deployment-specific
   secrets still come from env vars and override this base, as designed
   (see "Required env vars" below).

Do not skip fix #2 and go straight to creating a Render service — without it,
the app will deploy "successfully" (container starts, healthcheck may even
pass) but be functionally unconfigured.

---

## PRIORITY 2 — Apply this patch second (after Priority 1 and DB setup)

**File:** `0002-priority2-superposition-and-pooler-warning.patch` (repo root,
alongside this file)

Found while actually standing up the first live Render deploy after Priority
1 + a real Supabase Postgres were in place. Same `git am` convention:

```bash
git am 0002-priority2-superposition-and-pooler-warning.patch
# or, if that fails due to line-ending/context drift:
git am --3way 0002-priority2-superposition-and-pooler-warning.patch
```

### Correction to this file: Superposition is NOT optional at boot

Priority 1's "Explicitly out of scope" list below states Superposition is
"feature-flagged/optional, not required to boot the server." **That's
wrong** — confirmed by an actual boot panic:

```
thread 'main' panicked at crates/router/src/routes/app.rs:536:18:
Failed to initialize superposition client: Failed to initialize Superposition client
╰─▶ Configuration error: Both primary and fallback config fetch failed.
    Primary: Network error: Failed to fetch config: dispatch failure.
    Fallback: Configuration error: Failed to read config file
    "./config/superposition_seed.toml": No such file or directory (os error 2)
```

`app.rs` calls `.expect(...)` on the client init (line 536) — there is no
disable flag in `SuperpositionClientConfig`, so this is unconditional on
every boot, standalone container or not.

Root cause, two layers:
1. **Primary fetch fails** — baked config points `superposition.endpoint`
   at `http://superposition:8080`, another docker-compose-only service name
   with the same problem as the `pg` host issue Priority 1 fixed for the
   database. No such service exists for a standalone Render deploy.
2. **Fallback fails too** — `backup_file_path` in the baked config is the
   *relative* path `"./config/superposition_seed.toml"`. The Dockerfile
   never copies `superposition_seed.toml` into the image at all, and the
   final `WORKDIR` (`${BIN_DIR}`) isn't the repo root, so even a correct
   relative path couldn't resolve at runtime.

**The fix:** bake `superposition_seed.toml` into the image at the same
`${CONFIG_DIR}` used for the other baked config files (an absolute,
always-resolvable path), immediately after the existing
`payment_required_fields_v2.toml` COPY line. This makes the *fallback*
path succeed once the (still-unreachable) primary HTTP fetch fails, so
`provider.init()` returns Ok via the file data source instead of erroring
out entirely.

**Required env var** (add to the Render service, same pattern as the DB
vars in Priority 1 point 4):
- `ROUTER__SUPERPOSITION__BACKUP_FILE_PATH` = `/local/config/superposition_seed.toml`
  (i.e. `${CONFIG_DIR}/superposition_seed.toml` with `CONFIG_DIR`'s actual
  default value substituted in — confirm against the Dockerfile if
  `CONFIG_DIR` is ever overridden at build time).

### Decision: hybrid pooling — transaction mode for data pools, session mode where the protocol requires it

The earlier draft of this section treated the port-6543 switch (made to
dodge Supabase session-mode's 15-connection cap — `FATAL: max clients
reached in session mode`) as an unresolved, possibly-unsafe stopgap. It's
now a deliberate, resolved design, split by what each connection actually
needs — not "everything on one port":

**On Supabase's transaction-mode pooler (port 6543):**
`MASTER_DATABASE`, `REPLICA_DATABASE`, `ACCOUNTS_DATABASE`,
`GLOBAL_DATABASE` — the four Diesel-backed data pools that were hitting the
connection cap. Diesel's default behavior of caching *named* prepared
statements client-side is genuinely unsafe under transaction-mode pooling
(the pooler reassigns the real backend Postgres connection per transaction,
so a statement prepared on one backend can be executed against a different
one later — intermittent "prepared statement ... does not exist" errors
under load, confirmed via Supabase's own docs and the upstream Diesel
issue tracker). This is now fixed at the code level, not worked around:
`crates/storage_impl/src/config.rs` gained a
`disable_prepared_statement_cache: bool` field on `Database` (default
`false`), and `crates/storage_impl/src/database/store.rs`'s pool builder
now calls `Connection::set_prepared_statement_cache_size(CacheSize::Disabled)`
on every new pooled connection when that flag is set. This makes Diesel
fall back to Postgres's unnamed-statement, single-message extended-query
protocol (prepare+execute together, per upstream Diesel PR #4539) — the
exact pattern transaction-mode poolers are designed to support safely.
**Required env var**, one per section, in addition to the
`HOST`/`PORT`/`USERNAME`/`PASSWORD`/`DBNAME` vars already set:
- `ROUTER__MASTER_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true`
- `ROUTER__REPLICA_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true`
- `ROUTER__ACCOUNTS_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true`
- `ROUTER__GLOBAL_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true`

**Stays on Supabase's session-mode pooler (port 5432):**
`ROUTER__REDIS__POSTGRES_URL` — this was already correct in the original
Priority 1/pre-session guidance (point 3, below) and does not change. This
URL backs the Postgres-based Redis/KV replacement, which uses `LISTEN`/
`NOTIFY` for the pub/sub sweep jobs (`pg_pub_sub.rs`) and needs a
persistent, stable backend connection for that to work at all — transaction
mode tears the backend connection down between transactions, which breaks
`LISTEN`/`NOTIFY` regardless of the prepared-statement question. Disabling
the statement cache would not fix this one; it needs an actual pinned
session, so it's not a candidate for the transaction pooler at all.

**Wired (2026-10-09):** schema migrations need session mode or a direct
connection. Diesel migrations take advisory locks and run DDL, both of which
are session-scoped and will not work reliably through a transaction-mode
pooler. A distinct migration-only connection string now exists:
`MIGRATION_DATABASE_URL` (falls back to `DATABASE_URL` when unset, so
single-URL setups are unaffected). `just migrate` / `migrate_v2` /
`migrate_v2_compatible` use it, `scripts/migration_runner_entrypoint.sh`
prefers it over `DATABASE_URL`, and both compose files forward it to the
`migration_runner` service. Point it at a session-mode/direct URL (port
5432, `?sslmode=require`) while the app's steady-state pools stay on 6543;
don't reuse the `MASTER_DATABASE` env vars for migrations as-is.

If a future session needs to raise the session-mode connection ceiling
directly instead of splitting by pooler mode (e.g. a paid Supabase tier
with a higher cap), that's still a valid alternative to revisit — but the
hybrid split above is the current, working, verified-safe-by-design state,
not a stopgap.

---

## Context: why this investigation happened

- `Zapier-codes/B-Pay-backend` is a **fork** of `Phoenix-Boss/B-PAY-backend`
  (same org, different accounts). There is one open PR back to upstream and
  one fork, confirmed via the upstream repo page.
- Upstream's `main` branch used to be a small Node.js webhook-gateway app
  (`index.js`, `routes.js`, `webhookGateway.js`, `providers/`, `utils/`,
  a `render.yaml`). Zapier-codes' PR replaces this with the **Hyperswitch**
  Rust payment-orchestration engine (large Cargo workspace under `crates/`).
- The Node app was **not deleted** originally — it was parked in a
  dedicated subfolder of this repo, complete with its own `render.yaml`
  and `handover.md`. (That subfolder has since been **removed entirely**:
  all ten providers are now native Rust connector crates, so the old
  codebase is gone.) That folder's `render.yaml` listed the env vars
  for the *old* system
  (`PAYSTACK_SECRET_KEY`, `KORAPAY_SECRET_KEY`, `JUICYWAY_SECRET_KEY`,
  `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY`, `MAVW_WEBHOOK_URL`,
  `MAVW_WEBHOOK_FORWARD_SECRET`, `INTERNAL_API_KEY`). **None of these are
  read by the Rust app** — confirmed by grepping the whole `crates/` tree
  for every one of those key names: zero matches. Decision from this
  session: **abolish the old system, focus only on the new (Rust)
  infrastructure.** Do not copy those old Node env vars into the new
  Render service.
- Upstream (`Phoenix-Boss/B-PAY-backend`) currently has its own Render
  service (`srv-d6cv1ssr85hc73bh0ot0`, configured as `env: node`, plan
  `free`, region `oregon`) running the old Node app. Goal: give
  `Zapier-codes/B-Pay-backend` its **own, separate** Render service running
  the new Rust system, so both forks run independently.

## What the CI failure actually was (already understood/fixed pre-session)

The `build-and-push` job's Docker build/push succeeded fine. The final step,
"Trigger Render deploy hook", failed the whole job with exit code 1 because
`RENDER_DEPLOY_HOOK_URL` wasn't set as a GitHub Actions secret on
`Zapier-codes/B-Pay-backend` (makes sense — there was no Render service for
this fork yet to have a deploy hook from). See Priority 1 fix #1 above.

## Render/API access already set up (Termux, not this sandbox)

Session established, via Termux on the operator's device (**not** persisted
in this sandbox — re-establish per new session if needed):

- `RENDER_API_KEY_OLD` — key for the account owning the **upstream**
  service (`Phoenix-Boss/B-PAY-backend`, `srv-d6cv1ssr85hc73bh0ot0`).
- `RENDER_API_KEY_NEW` — key for the account that will own the **new**
  service for `Zapier-codes/B-Pay-backend`. Owner ID confirmed:
  `tea-danrh6rm8hqs73ca4s5g` ("My Workspace" team).
- Both persisted in `~/.bashrc` on the operator's Termux.
- The old service's env vars were pulled and saved to
  `~/.render_env_old.sh` on Termux, masked-verified present — **but per the
  decision above, these are the old Node app's vars and should NOT be
  copied into the new service.** Leave that file alone; it's not needed for
  the new-system path.

## What's actually needed before creating the new Render service

1. **Apply `priority1.patch`** (above) and push to `Zapier-codes/B-Pay-backend`.
2. **A real Postgres database.** The Rust app has no working default here —
   `ROUTER__MASTER_DATABASE__*` must point at something real. Options:
   Render's own managed Postgres (simplest, same network), a new Supabase
   project, or reuse/branch an existing one. **Not yet decided — first
   question for the next session to resolve with the operator.**
3. **Decide the KV/cache backend.** The Dockerfile's `KV_BACKEND` build arg
   defaults to `postgres` (via the `redis_interface` crate's Postgres
   backend — see `crates/redis_interface/README.md`), which avoids needing
   a separate Redis server entirely. If keeping this default: apply
   migrations `2026-09-11-120000_add_postgres_kv_replacement` (required)
   and optionally `..._130000_add_pg_kv_cache_pubsub_sweep_jobs`, then set
   `ROUTER__REDIS__POSTGRES_URL` to a **session-mode/direct** Postgres
   connection string (port 5432, `?sslmode=require` — not the 6543
   transaction-pooler port, which lacks prepared-statement support).
4. **Minimum required env vars** for the new Render service (`ROUTER__`
   prefix, `__` separator — see `config::Environment::with_prefix("ROUTER")`
   in `crates/router/src/configs/settings.rs`):
   - `ROUTER__MASTER_DATABASE__HOST` / `PORT` / `USERNAME` / `PASSWORD` /
     `DBNAME`
   - `ROUTER__REDIS__POSTGRES_URL` (if using the postgres KV backend — see
     point 3)
   - `ROUTER__SECRETS__ADMIN_API_KEY`
   - `ROUTER__SECRETS__JWT_SECRET`
   - `ROUTER__SECRETS__MASTER_ENC_KEY`
   - `ROUTER__SERVER__PORT` — **resolved, no action needed (2026-09-25):**
     this was flagged as unhandled, but Render's own support forum
     (community.render.com, staff reply) confirms Docker/image-backed Web
     Services don't require the container to read a Render-injected `$PORT`
     at all — Render does its own port *detection* against whatever port
     the container is actually listening on, and `PORT` is only needed to
     skip/speed up that detection or to force a specific port when
     detection picks the wrong one among several. Since the image `EXPOSE`s
     `8080` (`Dockerfile` line 122) and `docker_compose.toml`'s baked
     `server.port = 8080` is the only port the app binds, Render's detector
     has exactly one candidate to find — consistent with both
     `b-pay-backend-new` and `control-center-new` already reporting healthy
     `/health` checks with no `PORT`-reconciliation code ever added. Nothing
     to change here; leave `ROUTER__SERVER__PORT` unset and let the baked
     `8080` default stand.
   These override the baked-in `docker_compose.toml` defaults from Priority
   1 fix #2; everything else in that file (locker mock, CORS, scheduler,
   connector filters, etc.) is usable as-is for a first working deploy.
5. **Create the Render service as an *image-backed* service, not a git-backed
   one.** (Corrected 2026-09-21 — this step used to say `"env": "docker"` with a
   `repo` and `branch`, which makes Render build the Rust workspace itself; the
   pipeline in this repo builds the image on GitHub and pushes it to GHCR
   instead.) The service that actually exists, `b-pay-backend-new`
   (`srv-daoal7btqb8s73eiu2qg`, owner `tea-danrh6rm8hqs73ca4s5g`), was verified
   through the Render API to be image-backed: `repo: null`, `runtime: image`,
   `imagePath: ghcr.io/zapier-codes/b-pay-backend:latest`. To create another one,
   use the dashboard's "Existing Image" flow with that image path and a GHCR
   registry credential (a GitHub token with `read:packages`), or the API
   equivalent — not `repo`/`branch`.
6. **Once created**, grab its deploy hook URL from the Render dashboard
   (or `GET /v1/services/{id}/deploy-hook` if available on the API) and add
   it as the `RENDER_DEPLOY_HOOK_URL` secret on `Zapier-codes/B-Pay-backend`
   in GitHub — this is what makes Priority 1 fix #1's step actually fire.

## Explicitly out of scope / not needed

- Do NOT reuse `Phoenix-Boss/B-PAY-backend`'s Render service, deploy hook,
  or env vars for the new service — separate, independent deployments is
  the goal.
- Do NOT wire up the ~500 optional lines of
  `config/deployments/env_specific.toml` (Apple Pay, Google Pay, Paze,
  Kafka analytics, AWS SES, S3, OIDC, gRPC routing/recovery microservices,
  etc.) for a first working deploy — all feature-flagged/optional, not
  required to boot the server. **Superposition is the one exception** —
  see PRIORITY 2 above; it is not optional and will panic the app on boot
  without the file-fallback fix.

---

## CORRECTION to PRIORITY 2/3 — `disable_prepared_statement_cache` does not work on the pinned diesel (2026-09-21)

> **RESOLVED 2026-10-01** by the diesel 2.3 bump — see "Session update (2026-10-01)" below. The text in this section is kept as history; its "flag has no effect" and "`bump-diesel-2.3` branch" statements are no longer true.

The hybrid-pooling section above says the transaction-pooler problem "is now fixed
at the code level" by `Connection::set_prepared_statement_cache_size(CacheSize::Disabled)`.
**That call does not exist in the diesel this workspace is locked to.** It was added
in diesel **2.3.0** (checked against the published 2.3.0 source: `enum CacheSize`
and `fn set_prepared_statement_cache_size` are present; the locked 2.2.10 has
neither). The Priority 3 commit (`1f9d6e8be`) was never compiled, and it broke
`main`: the `Build and Publish Docker Image` job and the CI "Cargo hack" job both
failed with three errors in `storage_impl` — `E0433 cannot find CacheSize in
connection` and `E0599 no method named set_prepared_statement_cache_size`
(`database/store.rs:285`), plus `E0063 missing field
disable_prepared_statement_cache` (`config.rs:95`, the `Default for Database`
initializer). (The image build itself got that far — i.e. compiled everything up
to `storage_impl` without running out of memory — which is what the earlier
`BUILD_JOBS`/swap change was for.)

**What was changed to make `main` build again:** the `disable_prepared_statement_cache`
field is kept (so `ROUTER__*_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true` is
still accepted and nothing crashes) and given its missing default (`false`); the
pool builder no longer calls the non-existent API and instead logs an **error** at
startup whenever the flag is set. **The flag therefore has no effect.**

**Consequence for the current design:** with the four Diesel pools on the 6543
transaction pooler, named prepared statements are still cached client-side, so the
intermittent `prepared statement "..." does not exist` risk described above is
real and unmitigated. Two ways out:
1. *Config only, works today:* put the Diesel pools back on the session pooler
   (port 5432) and stay under Supabase's 15-client cap with small pools, e.g.
   `MASTER_DATABASE__MAX_POOL_SIZE=4`, `REPLICA_DATABASE__MAX_POOL_SIZE=3`,
   `ACCOUNTS_DATABASE__MAX_POOL_SIZE=2`, `GLOBAL_DATABASE__MAX_POOL_SIZE=2`,
   `REDIS__POOL_SIZE=2` (13 total; add `..._MIN_IDLE_POOL_SIZE=1` per pool).
   These numbers are an estimate against the reported 15 cap — not measured.
2. *Code, a separate task:* make the flag real by bumping diesel to >= 2.3.0. The
   manifests already allow it (`^2.2.10`); the blocker is `deja`'s
   `DejaLoadConnection`, pinned by git rev, which wraps diesel's `Connection` and
   must implement the new trait method — the `juspay/deja` repo has a
   `bump-diesel-2.3` branch for this. Needs a `deja` rev change plus a full CI
   cycle; do not attempt it without a compiler.

### Follow-up to the correction above (2026-09-21): router had its own `Database`

Priority 3 added `disable_prepared_statement_cache` to `storage_impl::config::Database`
only. **`router` has a separate `settings::Database` struct** (with its own `Default` in
`configs/defaults.rs` and `impl From<Database> for storage_impl::config::Database`), which
was not updated: the `From` initializer failed with `E0063` at
`crates/router/src/configs/settings.rs:1229` — the next compile error after the
`storage_impl` ones, and what still broke the image build and four CI jobs after the first
fix. Now the field exists on router's struct, defaults to `false` and is forwarded, so
`ROUTER__MASTER_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true` (and the accounts/global/
replica variants) really reaches `storage_impl` — where it currently only logs an error, see
above. Before this fix the variable was silently ignored by serde. The drainer has its own
`Database` (no such field, no conversion) and is unaffected.

---

## Session update (2026-09-22): both services confirmed live; CI schedule disabled

### Both Render services are live and healthy, end to end

`b-pay-backend-new` (`srv-daoal7btqb8s73eiu2qg`) and `control-center-new`
(`srv-daobduf40ujc73el46r0`) both return `200`/`health is good` on `/health`
as of this session, running the image built from commit `96ed9ef32`
(current `origin/main` tip at time of writing). Getting here needed two
things beyond what's documented above:

1. **Both services must be image-backed, not repo-backed.** Creating a
   Render service with `repo`/`branch`/`dockerfilePath` makes *Render
   itself* clone and build from source on every deploy — completely
   bypassing the GHCR/buildx pipeline in `docker-publish.yml` (its caching
   is then dead weight, never used). The correct source config, confirmed
   via `PATCH /v1/services/{id}` with an `image: {ownerId, imagePath}`
   body: `imagePath: ghcr.io/zapier-codes/b-pay-backend:latest` /
   `ghcr.io/zapier-codes/control-center:latest`. This also incidentally
   fixes an unrelated bug: `control-center`'s `package.json` has a
   `postinstall` script (`git config core.hooksPath ...`) that fails with
   `fatal: not in a git directory` when Render builds from source (its
   build context has no `.git`), but works fine under GitHub Actions'
   `actions/checkout`, which does include one.
2. **The baked `docker_compose.toml` defaults four separate DB roles to
   `pg`** (a docker-compose-only hostname): `master_database` (already
   fixed, Priority 1), plus `accounts_database`, `global_database`, and
   `replica_database`, which were missed. All three needed the same
   Supabase host/port/user/password/dbname as master, added as
   `ROUTER__ACCOUNTS_DATABASE__*` / `ROUTER__GLOBAL_DATABASE__*` /
   `ROUTER__REPLICA_DATABASE__*` env vars on the Render service. (Our
   Supabase connection is on port `5432`, the session-mode pooler, so the
   transaction-pooler/prepared-statement problem described just above this
   section does not apply to this deployment — that issue is specific to
   port `6543`, which we never switched to.)

Current full set of env vars on `b-pay-backend-new`, beyond what's already
listed under Priority 1 point 4: the `ACCOUNTS_DATABASE`/`GLOBAL_DATABASE`/
`REPLICA_DATABASE` quintuples above, plus
`ROUTER__SUPERPOSITION__BACKUP_FILE_PATH=/local/config/superposition_seed.toml`
(required per the Superposition section above once that file started being
baked into the image).

### `control-center` deployment specifics (new this session)

- Render service is separate from the backend, `env: image`, same owner
  account, pointed at `ghcr.io/zapier-codes/control-center:latest`.
- Its own `docker-publish.yml` was added (mirrors the backend's, same
  `type=gha`/`mode=max` layer caching), pushing to GHCR under the
  `control-center` package.
- **Port is hardcoded to `9000`** in `src/server/Server.res` — not
  configurable via an app-level env var like the backend's server port
  was. Render's own `PORT` env var (distinct from any `ROUTER__`-style
  app config — this one is Render's platform-level port-binding variable)
  is set to `9000` to match, since Render's docs confirm it's a first-class
  configurable override, default `10000`.
- Connects to the backend via `default__endpoints__api_url` (this app's own
  `section__key` env-override convention, confirmed in its
  `start:test` npm script), set to
  `https://b-pay-backend-new.onrender.com/api`.
- `/health` route exists and was used as the healthcheck path, same as the
  backend.

### CI: nightly-tag/Postman-collection workflow schedule disabled

`.github/workflows/release-nightly-version.yml` ran on a weekday-midnight
cron and failed every time on this fork — it needs an `AUTO_RELEASE_PAT`
secret (a PAT with push access to `main`) that was never set, so
`actions/checkout`'s `token` input got an empty string and errored
immediately (`Input required and not supplied: token`). Investigated
before disabling rather than assumed: this workflow does two things,
neither of which is API documentation — (1) regenerates Postman
*test-collection* JSON files (`postman/collection-json/*.json`, used by the
separate `postman-collection-runner.yml` connector-testing workflow) from
their source directories and auto-commits the result to `main`, and (2)
creates a calendar-versioned git tag (`YYYY.MM.DD.MICRO`) via `git-cliff`.
Neither is consumed by anything else in this fork's pipeline — notably,
the tag format doesn't match `docker-publish.yml`'s `v*.*.*` tag trigger,
so it was never going to kick off a deploy even if it worked. **Decision:
disabled the `schedule:` trigger, kept `workflow_dispatch`** so it's still
runnable by hand later if ever wanted (at which point `AUTO_RELEASE_PAT`
would need to be added as a secret too — not done, out of scope for this
fix).

---

## Session update (2026-09-25): PORT-reconciliation open item resolved

- **Closed the `ROUTER__SERVER__PORT`/Render-`$PORT` open item** from
  "What's actually needed before creating the new Render service," point 4
  above — see that bullet for the finding (Render does its own port
  detection for Docker/image-backed services; no code or env-var change
  needed). This had been sitting as "not yet resolved" since the original
  investigation even though both services have been confirmed live and
  healthy since the 2026-09-22 update — worth closing explicitly so a
  future session doesn't re-open it as a live risk.

---

## Session update (2026-10-01): diesel 2.3 bump — `disable_prepared_statement_cache` is real now

Picked as the next task (landing page lives in another repo): the only remaining item with a named blocker.

**The blocker, restated.** `juspay/deja` had merged a diesel-2.3 bump (PR #91), reverted it (PR #95: deja must not move ahead of its host), and re-raised it as `reland/diesel-2.3-bump` (the `bump-diesel-2.3` branch named above no longer exists). `deja` waited on the host, the host waited on `deja`. This change breaks the cycle by moving them together.

**What changed**
- `deja` pin `de0a42a42` -> `3a59544ae7e4064c893366f283e6e8f7c48270ab` (tip of `reland/diesel-2.3-bump`; its wrapper implements `set_prepared_statement_cache_size`). All 9 occurrences.
- `diesel` 2.2.10 -> 2.3.0 (locks to 2.3.13), `async-bb8-diesel` 0.2.1 -> 0.3.0, `bb8` 0.8 -> 0.9 (locks to 0.9.1), in every manifest that names them. `diesel_migrations` follows to 2.3.2 via the lockfile.
- **MSRV 1.85.0 -> 1.86.0** (every diesel 2.3.x declares 1.86.0): `Cargo.toml`, `.deepsource.toml`, the pinned `storage-impl` job in `ci.yml`. The Docker build uses `rust:trixie` and is unaffected.
- `storage_impl/src/database/store.rs`: `TestTransaction` replaced by one composed `ConnectionSetup` customizer (bb8 allows a single customizer per pool) that applies `CacheSize::Disabled` and/or the test transaction. bb8 0.9 dropped `#[async_trait]` on `CustomizeConnection`, so it is written against the new `Pin<Box<dyn Future>>` signature. The "set but NOT supported" error log is gone; no customizer is installed when neither option is on.
- `storage_impl/tests/transaction_pooler.rs`: `#[ignore]`d regression test against a real transaction-mode pooler (run instructions in its header). Needs the `tokio` `macros` dev-dependency added to `storage_impl`.

**Verification — read this before trusting it**
- Verified in the sandbox: lockfile resolution; and a standalone crate containing `ConnectionSetup` verbatim, built on bb8 0.9.1 / async-bb8-diesel 0.3.0 / diesel 2.3.13 (compiles, no errors), run against Postgres 16 behind PgBouncer 1.22 (`pool_mode=transaction`, `max_prepared_statements=0`, pool 2, 8 clients x 60 rounds x 3 statements): direct, cache on: 480 ok / 0 failed; pooler, cache on: 131 ok / 349 failed (`prepared statement "__diesel_stmt_2" already exists`); pooler, cache off: 480 ok / 0 failed.
- **NOT verified: the real workspace build.** The sandbox (~4 GB RAM cap) OOM-kills rustc on `diesel` with `128-column-tables`, so `cargo check -p storage_impl`, the `release` feature set (which compiles `deja`), and the in-repo test were never run. The standalone probe used plain `PgConnection`, not `deja::DejaLoadConnection` (the forwarding is a 2-line delegation in deja, read but not compiled here). Treat the PR's CI as the first real compile signal; do not push to `main` unreviewed (the Priority 3 commit broke `main` exactly this way).

**Consequences for deployment**
- `ROUTER__*_DATABASE__DISABLE_PREPARED_STATEMENT_CACHE=true` now has an effect, so the four Diesel pools can go on the 6543 transaction pooler safely; `ROUTER__REDIS__POSTGRES_URL` must stay on 5432 (LISTEN/NOTIFY).
- Open: `get_database_url` sends `options=-c search_path=<schema>`. PgBouncer rejects it unless `ignore_startup_parameters = options`; not checked against Supabase's pooler (`public` is the default schema, so possibly harmless — test it).
- Resolved (2026-10-09): a distinct session-mode/direct URL for migrations is
  now wired as `MIGRATION_DATABASE_URL` (falls back to `DATABASE_URL`). See the
  2026-10-09 session update below.

---

## Session update (2026-10-09): `main` CI actually failed on five jobs, not one

The previous section's "already understood/fixed pre-session" note only covered
`build-and-push` / the Render deploy hook. The real `main` run
(`37849716352`) failed **five** jobs, all but the first caused by a single
`juspay/deja` API change not yet threaded through:

1. `Run tests on stable toolchain` (`just clippy`) — the gating job.
2. `Check compilation on MSRV toolchain` (`ci_hack`).
3. `Check compilation for V2 features`.
4. `Check formatting`.
5. `build-and-push` — unchanged cause (missing `RENDER_DEPLOY_HOOK_URL`).

**Fixes applied**
- **E0061 (7 args vs 6)** — `deja`'s `replay` gained a 7th argument; added it at
  the two (three call paths) sites: `external_services/src/grpc_client/
  deja_transport.rs` and `external_services/src/superposition.rs`. Clears jobs
  1–3.
- **`clippy::trivially_copy_pass_by_ref`** — `storage_impl/src/database/store.rs`.
- **`semicolon_in_expressions_from_non_local_macros`** — the `type_name!` macro
  in `common_utils/src/macros.rs` ended in `;`, which is a future-compat hard
  error in expression position. E0061 had previously aborted the build before
  this lint was reached, so it surfaced only once the above were fixed. Removed
  the stray semicolon (semantically neutral at all 178 call sites).
- **Formatting** — `cargo +nightly fmt --all` over `korapay.rs` and
  `korapay/transformers.rs`.
- **Migration URL** — new `migration_database_url` in `justfile` (default
  `env_var_or_default('MIGRATION_DATABASE_URL', database_url)`) used by
  `migrate` / `migrate_v2` / `migrate_v2_compatible`; `MIGRATION_DATABASE_URL`
  forwarded by both compose files' `migration_runner`; and
  `scripts/migration_runner_entrypoint.sh` prefers it over `DATABASE_URL`.

**Still open:** `build-and-push` still needs `RENDER_DEPLOY_HOOK_URL` set as a
repo secret. Also `get_database_url` sends `options=-c search_path`, which some
poolers reject (unchanged).

---

## Session update (2026-10-09, part 2): clippy sweep finishes green; the memory wall has a sustainable fix

Continues the session above. The E0061 fix unblocked compilation, which then
exposed a tail of lints that the earlier abort had masked. All are now fixed and
`just clippy` (default and `fred`) pass **exit 0** in the sandbox — the first
green clippy on this tree.

**Lints cleared** (all denied by the workspace lint set under CI's `-D warnings`)
- `semicolon_in_expressions_from_non_local_macros` — the `fallback_reverse_lookup_not_found!`
  transcriber ended in `;` (hit at 10 `storage_impl` sites); removed.
- `derivable_impls` — `NovuClient` gets `#[derive(Default)]` instead of a manual `impl`.
- unused imports — `PaymentsSyncRouterData` (opik), `ConnectorAuthType` (prestmit).
- `clippy::todo` — `#[allow]` on the external-vault-proxy `postprocessing_steps`
  stub (the only caller is the OpenBanking/Plaid path this flow never takes;
  mirrors the repo's existing precedent).
- `useless_conversion` — redundant `.into_iter()` in `psync_flow.rs`.
- `block_scrutinee` — hoisted the `if let Some(x) = { .. }?` scrutinee to a local.
- `large_futures` — `Box::pin` the `authentication_authenticate_core` future.
- `clippy::panic` + `clippy::as_conversions` — in the `transaction_pooler`
  regression test (`--all-targets` lints tests too).

**The OOM wall — sustainable fix, not "all the RAM"**

The recurring failure is one rustc invocation: `diesel` with `128-column-tables`
(required by `diesel_models`/`common_enums`/`euclid`) peaks at **~9.8 GiB** in a
debug/no-debuginfo profile. It is a **frontend** spike (metadata emission), so
`codegen-units`, `opt-level`, and even `strip=debuginfo` don't shrink it — only
`-j1` and fewer debug knobs help. Two things make it affordable instead of a
per-build crisis:

1. **Tune the dev profile down.** `CARGO_PROFILE_DEV_DEBUG=0` plus
   `CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false`/`OVERFLOW_CHECKS=false` keeps the
   peak under ~10 GiB (was >13 GiB with debuginfo). One-time cost.
2. **Persist the caches.** Run with the target dir, `~/.cargo/registry`, and
   `~/.cargo/git` all bind-mounted **under `/workspace`** (a Docker volume is
   wiped when the daemon/sandbox restarts — that happened mid-session and cost
   the whole cache). Once diesel is compiled it is reused, so every later run is
   ~1 GiB and fast: the second `just clippy` finished in seconds.

CI does not need any of this — it has a multi-GB runner and its own cache; this
is only for reproducing the lint locally in the ~15 GiB sandbox.

**Sandbox note:** the sandbox restarted once during a 13 GiB container run
(host has 15 GiB, no swap) — which is exactly the argument against sizing the
build to "all our RAM". Also, `sudo` is required for `docker` after a restart.

**Blocker — cannot push.** Both the repo-embedded token and `$GITHUB_TOKEN`
authenticate as `Zapier-codes` but are **read-only** (empty OAuth scopes; API
ref creation and PR creation both return 403). The commits are on local `main`
and are handed off as a patch (same convention as Priority 1/2 above).

**Handoff patch:** `0003-main-ci-clippy-and-format-fix.patch` (repo root). It is
a `git format-patch` mailbox of **four** commits — the three CI fixes
(`48bfcadeb`, `3a1dba12b`, `741a1fce7`) plus this handover update. Apply and
push with:

```bash
git am 0003-main-ci-clippy-and-format-fix.patch
# or, if that fails due to line-ending/context drift:
git am --3way 0003-main-ci-clippy-and-format-fix.patch
git push origin main
```

(`git am` keeps the original commit messages/authors; use `git apply` only if
you want the changes uncommitted. `git am *.patch` also works if the four
`000N-*.patch` series files are used instead of the combined mailbox.)

---

## NEW TASK — Industry landing page (Stripe-style), first pass built (2026-09-22)

Requested 2026-09-22. Goal: a polished, animated marketing/landing page for
this product, matching the **quality bar and UX conventions** of Stripe's
own landing page — generous whitespace, confident large typography, a
hero section with subtle ambient motion (gradient/mesh animation or
similar), scroll-triggered reveals, and an interactive code-snippet
showcase demonstrating the API. **Important scope boundary, worth
restating to whichever session picks this up:** this means matching the
*genre* and *polish level* of that style of fintech landing page — original
copy, original visual assets, this product's own branding and feature set.
It does not mean scraping or reproducing Stripe's actual page content,
layout code, copy text, or trademarked assets — that's not something to
attempt regardless of how the task is phrased in a future prompt.

Split into subtasks so a session can pick up one at a time rather than
needing to do all of this in one pass. **Subtasks 1–6 have a first pass
built** (own repo, not yet part of this one — the code lives outside
`Zapier-codes/B-Pay-backend`, so nothing here changed to add it); subtask 7
is intentionally left placeholder-quality. Status per subtask below.

### 1. Decide where this lives (repo + hosting) — do this first

**Went with option A (dedicated repo)** — matching the established pattern
of `b-pay-backend-new`/`control-center-new`, for consistency. **Not
confirmed with the product owner** the way this section originally asked
for — the session that built this asked, got "use industry giants style to
answer all questions" back rather than an actual A/B choice, and proceeded
with the dedicated-repo default this section itself already called
"probably right." Treat this as a reasonable default, not a
product-owner-confirmed decision — worth a real confirmation before this
goes further (e.g. before spending real effort on subtask 7's copy pass,
or on wiring subtask 6's CI/CD to an actual Render service).

The repo itself was built in a sandbox and has not been pushed anywhere
real yet — it doesn't exist at `Zapier-codes/landing-page` or anywhere else
on GitHub. What exists is a local git repo (one commit, `3e62638`) that
needs `git remote add origin <url>` and a push once that repo is actually
created — same "sandbox commits aren't pushed anywhere" situation as every
other patch in this file, except this is a whole new repo rather than a
patch onto this one.

### 2. Pick the stack — done

Vite + React + TypeScript, fully static output (`npm run build` verified
in-sandbox; no Rust/backend runtime).

### 3. Design system: original, not copied — done, first pass

Ink `#12151b` / signal `#5eead4` (teal) / reroute `#f5a623` (amber) — the
two accents appear only inside the hero's routing diagram, not as page
decoration. Space Grotesk (display) + IBM Plex Sans (body) + IBM Plex Mono
(code). See the landing-page repo's own README for the full design plan
and rationale.

### 4. Hero section + ambient animation — done, different approach than "gradient mesh"

Built as a small live diagram of the product's actual mechanism (a decline
at one processor, instant reroute to a working one) instead of a
decorative gradient/particle effect — CSS + SVG `animateMotion`, no
canvas/WebGL, GPU-cheap. Deliberately not a generic gradient mesh: the
diagram *is* the product's value proposition, not ambient decoration.

### 5. Scroll-triggered content sections — done, restrained

Feature highlights (route/retry/observe) and the code showcase each reveal
once via `IntersectionObserver`, as a single section-level moment —
deliberately not a per-card fade-slide-up on every element, which reads as
a generic AI-generated tell.

### 6. CI/CD wiring — written, untested

`docker-publish.yml` replicates the established pattern (GHCR,
`type=gha`/`mode=max` caching, image-backed Render deploy hook) exactly,
but has never actually run — there's no real GitHub repo or Render service
for it to run against yet. Do subtask 1's real repo creation and Render
service setup before trusting this workflow works as written.

### 7. Content/copy pass — intentionally left placeholder-quality

Copy describes a placeholder product name ("Routeway") and its routing/
retry/observability features in plain language, written fresh (not adapted
from Stripe's or anyone else's copy) — but it's demonstration copy for the
page structure, not this product's actual final name, brand voice, or
feature list. Needs a real pass with actual product/brand input, and the
placeholder name replaced everywhere, before this ships.

### Also not done: visual/screenshot QA

This sandbox has no headless browser — `npm run build` succeeding is the
only verification that happened. Load it locally and actually look at it
before treating the visual design as final.

---

## NEW TASK — Unregistered business: 1-year full-access grace period

Requested 2026-09-22, as a specific refinement of the general tiered
KYC/KYB threshold model described in the Product Vision section above.
**Read that section first** — this task changes what "the threshold"
means for one particular segment of users; it does not replace the
general model for everyone else. Not started, no code exists for this.

### The rule, precisely
A business with **no business registration on file at all** (no
CAC/Ltd/LLC-equivalent registration submitted — the unregistered end of
the spectrum, distinct from an individual who has done baseline KYC) gets
**the full system, fully, for exactly one year from account creation** —
sending and receiving live payments with no amount cap, no restricted
features, the complete normal experience of a fully verified account.
This is more generous than the general amount-based threshold described
in the Product Vision section, which still applies to other
tiers/segments — this is a distinct, time-boxed rule specific to this
segment, not a description of the general model.

### The deadline must be genuinely invisible, not just unstated
This is the important, easy-to-get-wrong part: the user must have **no
way to perceive** that a countdown exists — no visible expiry date
anywhere in Control Center, no "X days remaining" indicator, no early
warning email hinting at an upcoming change, nothing in any API response
that reveals the cutoff. The experience should be indistinguishable from
a permanently fully-verified account, right up until the exact moment of
suspension. This is a stronger requirement than the general threshold
model's "admin-only visibility" — there, at least the *existence* of a
threshold concept could reasonably leak without breaking the design;
here, even that should not be inferable. Build/test this deliberately —
e.g. no debug logging, no admin-facing UI element, that a user could ever
glimpse (shared browser, screenshot, support ticket, etc.).

### At exactly 1 year: suspend, notify, explain
Same suspension + Novu-email mechanism as the general model (same
`UserStatus` extension work, same suspended-state design — do not build
a second, parallel suspension mechanism for this case). The email at this
specific moment should clearly explain *why* — this account has been
operating on an initial grace period, and KYB is now required to
continue — since, unlike the general model's amount-triggered
suspension, this one wasn't preceded by any visible signal at all, so the
explanation carries more of the burden of feeling reasonable rather than
sudden.

### Completing KYB lifts it, same as the general model
Submitting full KYB (business registration, beneficial ownership — see
the KYC/KYB document-storage design task for where/how this data itself
gets stored) removes the restriction entirely, same mechanism as the
general model's KYB tier.

### Clock start: resolved — account creation
Confirmed by the product owner (2026-09-22): the 1-year clock starts at
**account creation**, not first live transaction. A dormant account
(created but never transacted) still gets suspended at the 1-year mark
same as an active one — no special-casing for dormancy. Store the
creation timestamp already implicit in the account record; no new field
needed beyond whatever the account-creation flow already timestamps.

---



## Task 77 — all ten Node.js providers ported to native Rust connectors; the old Node app is gone (2026-10-08)

This session completed the Task 77 migration. The ten original Node.js
providers now exist only as native Hyperswitch connector crates under
`crates/hyperswitch_connectors/src/connectors/`: Korapay, Paystack,
JuicyWay, Flutterwave (the first five, done in prior sessions) and Remita,
DodoPayments, PaymentPoint, Xixapay, Prestmit, `telcos.opik.net`
(the second five, completed here).

The system is a full Hyperswitch fork; the old Node.js gateway is no longer
part of this repo at all. Nothing here reads, imports, or depends on it —
the port is complete and the Node app has been removed entirely.

What was finished this session:

- **Prestmit** — replaced the placeholder scaffold with a real gift-card
  SELL-trade implementation: `POST /partners/v1/giftcard-trade/sell/create`
  for Authorize and `GET /partners/v1/giftcard-trade/sell/history?referenceOrID={ref}`
  for PSync (Prestmit has no single-trade GET). Auth is `API-KEY` +
  `API-Hash` where the hash is HMAC-SHA256 of `{API_KEY}:{json_body}` hex,
  computed at request time from the exact serialized body
  `RequestContent::Json` emits. Credentials carried via `SignatureKey`.
- **opik** — dropped the last "scaffold" wording from its metadata; the
  VTU-airtime implementation (Authorize `POST /api/v1/purchase/airtime`,
  PSync reconciled through `GET /api/v1/transactions`) is real.
- **Remita, DodoPayments, PaymentPoint, Xixapay** — verified already
  holding real, provider-specific implementations (no placeholder scaffolds
  remain).
- **Test auth plumbing** — PaymentPoint, Xixapay, and Prestmit were listed
  as `HeaderKey` in `crates/test_utils/src/connector_auth.rs` while their
  config TOMLs and implementations require three credentials (Bearer secret
  + api-key header + businessId/account PIN). Corrected all three to
  `SignatureKey` and filled in the matching `sample_auth.toml` entries.

Verified: `cargo check -p hyperswitch_connectors --features v1`,
`--features "v1,payouts"`, `-p test_utils`, and
`cargo check -p router --test connectors --features v1` all compile clean
(zero errors).

**The Node app is removed.** The ten providers were ported to native Rust
connectors, so the old Node codebase has been deleted in full. Every
reference to it — in CI workflow comments, `.typos.toml`, this file, and
the provenance comments inside connector source files — has been removed
or reworded so the repo no longer points at it. The only thing retained
from the port is the endpoint facts each connector needs, now recorded in
its own comments against the live provider documentation rather than
against a file that no longer exists.
