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
(upstream) keeps its own legacy Node.js service running untouched (see
`legacy-node/` and the "Context" section below). This fork,
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

**Also needs session mode or a direct connection (not yet wired, flag for
next session):** schema migrations. Diesel migrations typically take
advisory locks and run DDL, both of which are session-scoped and will not
work reliably through a transaction-mode pooler. Whatever runs migrations
against this database (`diesel migration run`, or an embedded harness at
startup) should point at a session-mode/direct URL even though the app's
steady-state pools are on 6543 — this repo doesn't yet have that wired up
as a distinct migration-only connection string; don't assume the
`MASTER_DATABASE` env vars are safe to reuse for migrations as-is.

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
- The legacy Node app was **not deleted** — it was moved to `legacy-node/`
  in this repo, complete with its own `render.yaml` and `handover.md`. That
  subfolder's `render.yaml` lists the env vars for the *old* system
  (`PAYSTACK_SECRET_KEY`, `KORAPAY_SECRET_KEY`, `JUICYWAY_SECRET_KEY`,
  `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY`, `MAVW_WEBHOOK_URL`,
  `MAVW_WEBHOOK_FORWARD_SECRET`, `INTERNAL_API_KEY`). **None of these are
  read by the Rust app** — confirmed by grepping the whole `crates/` tree
  for every one of those key names: zero matches. Decision from this
  session: **abolish the old system, focus only on the new (Rust)
  infrastructure.** Do not copy the legacy Node env vars into the new
  Render service.
- Upstream (`Phoenix-Boss/B-PAY-backend`) currently has its own Render
  service (`srv-d6cv1ssr85hc73bh0ot0`, configured as `env: node`, plan
  `free`, region `oregon`) running the legacy Node app. Goal: give
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
  decision above, these are the legacy Node app's vars and should NOT be
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
   - `ROUTER__SERVER__PORT` — reconcile against Render's injected `$PORT`
     (Render web services must listen on the port Render assigns; the app's
     own default is `8080` — needs explicit handling, not yet resolved).
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

## NEW TASK — Industry landing page (Stripe-style), not yet started

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

Not scoped or started yet beyond this breakdown. Split into subtasks so a
session can pick up one at a time rather than needing to do all of this in
one pass:

### 1. Decide where this lives (repo + hosting) — do this first
This product now has two existing services on the same Render account
(`b-pay-backend-new`, `control-center-new`), each with its own GitHub repo
and `docker-publish.yml` → GHCR → Render-image-pull pipeline (see Priority 1
and the Control Center section above for the exact pattern to replicate).
Decide: a third sibling repo/service (e.g. `Zapier-codes/landing-page`),
or a static bundle folder inside an existing repo? A dedicated repo
matching the established pattern is probably right, for consistency, but
confirm with the product owner before scaffolding it — not assumed here.

### 2. Pick the stack
Recommend a static-output framework (e.g. Next.js static export, Astro, or
plain Vite + React) rather than anything needing a Rust/backend runtime —
this is a marketing page, it doesn't need to talk to the router API for
anything beyond maybe a "get started"/signup link. Keep it a genuinely
static build so the Docker image is small and the deploy is fast (same
"aggressive caching, near-zero rebuild for small changes" goal already
established for the other two services applies here too).

### 3. Design system: original, not copied
Establish this product's own type scale, color tokens, and spacing system
before writing any page content — matching Stripe's *caliber* of design
system (consistent scale, restrained palette, purposeful motion) without
reusing their actual tokens/palette/logo. If this session's environment has
access to a design-system/Artifact tool or an established brand kit for
this product already, use that; otherwise establish one from scratch as
part of this subtask.

### 4. Hero section + ambient animation
The signature Stripe-landing-page element: a hero with subtle, continuous
background motion (gradient mesh, particle/wave effect, or similar) behind
the headline and CTA. Keep performance in mind — this should be
GPU-cheap (CSS-only or a lightweight canvas/WebGL effect), not something
that tanks mobile Lighthouse scores.

### 5. Scroll-triggered content sections
Feature highlights, connector/integration showcase, and an interactive
code-snippet block (e.g. tabbed request/response examples hitting this
product's actual API shape) that reveal/animate in as the user scrolls.

### 6. CI/CD wiring
Once a repo exists (subtask 1), replicate the established pattern exactly:
`docker-publish.yml` with `type=gha`/`mode=max` layer caching → GHCR →
Render service created as **image-backed** (`PATCH .../services/{id}` with
an `image` field — not `repo`/`dockerfilePath`, per the correction
documented above) → deploy hook → `RENDER_DEPLOY_HOOK_URL` secret on that
repo specifically.

### 7. Content/copy pass
Real copy describing this product's actual features, connectors, and
value proposition — written fresh, not adapted from Stripe's or any other
company's existing marketing copy.

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

## NEW TASK — Bill of Exchange (BoE) instrument crate, decentralized mode

Added 2026-09-22. New crate `crates/boe_instrument/` (picked up automatically
by the workspace's `members = ["crates/*"]` glob — no root `Cargo.toml`
change needed). Covers `hashing`, `crypto_signal`, `model`, and `template`
modules. Verified in a sandbox session (older apt-provided rustc 1.75, not
this repo's pinned 1.85.0 — see "Verification status" below) rather than
against this repo's actual CI; run `cargo test -p boe_instrument` for real
confirmation once this lands.

### What it does

Generates Bill of Exchange instruments — an unconditional order to pay a
sum, from a drawer to a drawee, in favor of a payee — as structured records,
with:

1. **Alphanumeric instrument ID** (`hashing::generate_instrument_id`) — a
   short, deterministic, HMAC-derived reference like `BOE-9F3K2N8QZR1A`.
   Human-facing; safe to expose to users and counterparties.
2. **Cryptographic signal** (`crypto_signal::derive_crypto_signal`) — the
   alphanumeric ID converted into a deterministic Ed25519 keypair via
   HKDF-SHA256 (keyed on the same server HMAC secret). The **public** half
   is the actual on-chain/smart-contract anchor — something a contract or
   third party can verify without trusting a central server, unlike the
   plain alphanumeric string. This crate does not talk to any chain; it
   only produces the signal. Anchoring it (event log, contract call, Merkle
   leaf, whatever the chosen chain uses) is a separate, not-yet-built layer.
3. **Session/device fingerprint** (`hashing::hash_session_signals`) —
   internal fraud/dedup signal only. Never rendered on the instrument,
   never used as a substitute for the drawer/drawee/payee's real identity.
4. **Tamper-evidence hash** (`hashing::content_hash`) — HMAC over the
   finalized legal fields, computed once at execution and never recomputed
   to "match" a later edit.
5. **Jurisdiction as a runtime enum**, including a `Decentralized` variant:
   no national legal system claimed; verified via the cryptographic signal
   and whatever smart-contract/platform terms the counterparties agreed to,
   instead of a court-recognized legal form. The other variants
   (`International`/UK/US/Nigeria/India) render their own legal wording and
   still get a `crypto_signal` attached for reference, but it isn't
   load-bearing for their enforceability the way it is for `Decentralized`.

### Two things worth being precise about before building on this

- **"No jurisdiction" ≠ "enforceable everywhere."** Decentralizing the
  verification mechanism doesn't grant a legal system's backing — it means
  enforceability rests entirely on the counterparties' contract/platform
  terms, which is narrower than what a national instrument gets from that
  country's courts. If cross-border *legal* recognition of an electronic
  instrument is the actual goal, the real mechanism is jurisdictions
  adopting the UNCITRAL Model Law on Electronic Transferable Records
  (MLETR) — e.g. the UK's Electronic Trade Documents Act 2023 — which is
  orthogonal to and combinable with the decentralized/signal-based
  verification built here, not a substitute for it.
- **Consent is a hard gate regardless of jurisdiction.** `execute_instrument`
  refuses to run unless `BillOfExchange.consent` is populated from a real
  user action (OTP confirmation, e-signature, etc.) — passive session/device
  tracking data is fraud-signal only and cannot authorize an instrument on
  its own. This applies identically in `Decentralized` mode; decentralizing
  who verifies the instrument doesn't change whether the drawer actually
  authorized it. Do not remove this gate.

### Verification status

`cargo test` run against `hashing.rs`, `model.rs`, `template.rs`, and the
HKDF-derivation logic in `crypto_signal.rs` — all passed, including
determinism checks (same instrument ID + key → same signal, every time) and
output-length checks (32-byte / 64-hex-char Ed25519 public keys). The
Ed25519 keypair construction itself (`SigningKey::from_bytes`,
`VerifyingKey`) and the signing/verification round-trip in `lib.rs` use the
correct ed25519-dalek 2.x API but were **not** runnable in this sandbox —
its apt-installed rustc 1.75 can't satisfy a transitive `edition2024`
requirement that ed25519-dalek 2.x's dependency tree pulls in, while this
repo's pinned `rust-version = "1.85.0"` should have no such problem. Run
`cargo test -p boe_instrument` here before merging to confirm the full
crate, not just the HKDF/hashing subset.

### Suggested next steps

- The smart-contract/anchoring layer itself (which chain, how
  `crypto_signal.public_key_hex` gets published/referenced on it) is
  intentionally out of scope for this crate and not yet designed.
- Add PDF rendering on top of `template::render_text` if a signable
  document (not just a stored record) is needed.
- Decide where `hmac_key` / any KMS-held signing key for the national-law
  path lives in this repo's existing secrets management, and reuse that
  rather than adding a new one.

---

## NEW TASK — Tokenize BoE instruments via ERC-3643 (T-REX), mapped to `crates/boe_instrument`

Added 2026-09-22, as the direct follow-on to the BoE instrument task above.
**Read that section first.** Documented, not yet built — no Solidity exists
in this repo yet, and nothing here has been deployed or tested on any chain.
This section is a spec for whoever picks it up, not a status report.

### Why ERC-3643 specifically, not plain ERC-20

A tokenized bill of exchange is a transferable claim to payment — in most
jurisdictions that makes it a regulated security/debt instrument regardless
of the chain it's on, and a plain ERC-20 has no way to stop it from being
transferred to a wallet that was never KYC'd or that's in a
sanctioned/excluded jurisdiction. ERC-3643 (reference implementation:
Tokeny's T-REX protocol, `github.com/TokenySolutions/T-REX`, GPLv3, also
mirrored at `github.com/ERC-3643/ERC-3643`) exists specifically to add that
check at the protocol level: every transfer is gated on the receiving
wallet's on-chain identity holding the right claims, not left to an
off-chain terms-of-service promise nobody enforces.

### The mapping from `boe_instrument` to T-REX's components

T-REX's own architecture (from its README): **ONCHAINID** (per-user identity
contract holding keys/claims), a **Trusted Issuers Registry**, a **Claim
Topics Registry**, an **Identity Registry** (wallet → verified identity),
a **Compliance** contract (checks each transfer against the rules), and the
**Security Token** contract itself.

- **`model::Party` → Identity Registry entry.** Each `Party` with a wallet
  (drawer, drawee, payee) gets its own ONCHAINID deployed and registered.
  `Party.full_name`/`address` stay off-chain-authoritative (in this crate's
  own record) — ONCHAINID doesn't need to duplicate them, only needs claims
  proving *this wallet* belongs to a KYC'd identity permitted to hold the
  token (jurisdiction/accreditation claims, issued by whichever Trusted
  Issuer this platform designates).
- **`crypto_signal.public_key_hex` → a custom claim, not a management/action
  key.** This needs to be said explicitly because it's the one place a naive
  mapping breaks: ERC-734 (which ONCHAINID is built on) keys are typed as
  ECDSA (secp256k1) for anything that can actually sign Ethereum
  transactions, and our `crypto_signal` is Ed25519 — it is **not**
  transaction-signing-compatible with an ONCHAINID management/action key as-
  is. The correct mapping is to register it as a **claim** (a new claim
  topic, e.g. `BOE_INSTRUMENT_SIGNAL`, whose data payload is the
  `public_key_hex`), signed by a Trusted Issuer, and verified via the Claim
  Topics/Trusted Issuers Registries like any other claim. This preserves the
  off-chain tamper-evidence binding (`content_hash` ↔ `crypto_signal`)
  without pretending the Ed25519 key is something it isn't on an EVM chain.
  If a future session wants the signal to double as an actual EVM signing
  key, that requires re-deriving it as a secp256k1 keypair instead
  (`k256`/`secp256k1` crate) — a different function from
  `crypto_signal::derive_crypto_signal`, not a reinterpretation of its
  current output.
- **`instrument_id` → the on-chain reference for a specific Security Token
  deployment (or a per-instrument mint), matching the human-facing label to
  its on-chain identifier.** Whether each BoE gets its own token contract or
  all BoEs share one contract with `instrument_id` as per-token metadata is
  an open design choice — not resolved here.
- **`content_hash` → emitted at mint time** (an event on the Security Token
  contract, or a claim on the instrument's own issuer identity) so the
  on-chain record and the off-chain `BillOfExchange` struct can be
  cross-checked independently later.
- **The consent gate carries through unchanged.** `execute_instrument`'s
  refusal to run without `ConsentRecord` stays the actual authorization
  check; minting only happens *after* `InstrumentStatus::Executed`, so
  nothing gets tokenized from an unconsented draft. This is a hard
  prerequisite, not a nice-to-have — do not wire minting to any earlier
  status.

### Implementation paths, not yet chosen between

1. **Vendor T-REX directly** (`git clone` it into a new top-level
   contracts/ directory, or as a git submodule) and write the mapping layer
   above as new contracts/scripts calling into it.
2. **AI-assisted scaffolding**: a tool called `forge-rwa` (PyPI:
   `forge-rwa`) takes a natural-language asset description and generates a
   deployment-ready ERC-3643 contract set. Could plausibly take this crate's
   `BillOfExchange` fields as its asset description input — unverified,
   nobody has tried this against `forge-rwa` yet.

### Licensing flag — resolve before vendoring anything

T-REX is dual-licensed: GPLv3, or a proprietary license available from
Tokeny. This repo's own crates are Apache-2.0
(`package.license = "Apache-2.0"` in the workspace `Cargo.toml`). Solidity
contracts calling into a GPLv3 dependency are a separate deployable
artifact from the Rust binary, not linked into it, so this is not the same
copyleft question as it would be for a Rust dependency — but it's still a
real licensing decision (what gets open-sourced, under what terms, if this
contracts layer is ever published) that whoever picks this up should
resolve deliberately, not by default.

### What's needed before any of this compiles or deploys

- A chain/testnet decision (which EVM chain this targets) — not made here.
- The actual Solidity contracts implementing the mapping above — none exist
  yet in this repo.
- A decision on per-instrument vs. shared token contract (see `instrument_id`
  mapping above).
- Legal sign-off on which jurisdictions' claims the Trusted Issuers Registry
  will actually recognize — this is the compliance decision the whole
  ERC-3643 structure exists to enforce, and it is a legal/business call, not
  a code default.
