# Handover — Zapier-codes/B-Pay-backend infrastructure

This file documents an infra investigation session (2026-09-20) into why CI was
failing and what's actually needed to get this repo running on its own Render
service. Read this fully before touching CI/Docker/Render config again — it
will save you from re-doing the same discovery.

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
