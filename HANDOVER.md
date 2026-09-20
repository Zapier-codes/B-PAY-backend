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
5. **Create the Render service** via `POST /v1/services` with
   `"env": "docker"` (NOT `"env": "node"` — the earlier draft command in
   this session's chat history used `node` by mistake, copying the
   upstream/legacy service's config; ignore that draft), `ownerId:
   "tea-danrh6rm8hqs73ca4s5g"`, `repo:
   "https://github.com/Zapier-codes/B-Pay-backend"`, `branch: "main"`.
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
  Kafka analytics, AWS SES, S3, OIDC, Superposition, gRPC
  routing/recovery microservices, etc.) for a first working deploy — all
  feature-flagged/optional, not required to boot the server.
