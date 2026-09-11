# Reseller API Documentation

This is the documentation for the **Reseller API** — the VTU (airtime/data)
and payment infrastructure product, currently live for Nigeria at
`https://telco.opik.net/api/v1`.

> **Where this fits alongside `handover.md`:** this repo (`B-Pay-backend`)
> tracks the Reseller API as item `a-10` in its own provider-discovery
> list — a separate product this codebase may integrate *with* as one of
> its ten payment providers, not the same running server. This `docs/`
> folder documents that separate product on its own terms, captured from
> its live Swagger UI, because it's the same business owner's
> infrastructure and the plan (see `handover.md`) is to grow it into the
> flagship white-label platform. Keep that distinction in mind when
> reading both documents side by side.

## Why this is split into files

Every module below lives in its own file so that adding a plan, a
network, or an entire country doesn't require touching unrelated files
or re-reading a 2,000-line spec to find the one endpoint that changed.
This is the same reasoning `handover.md` already applies to code —
applied here to documentation.

## Structure

```
docs/
├── README.md                          — this file
├── openapi/                           — machine-readable spec (source of truth)
│   ├── openapi.yaml                   — root: info, servers, security, tag list, path index
│   ├── paths/
│   │   ├── auth.yaml                  — POST /auth/register, POST /auth/login
│   │   ├── plans.yaml                 — GET /plans
│   │   ├── wallet.yaml                — GET /wallet, POST /wallet/deposit
│   │   ├── purchases.yaml             — POST /purchase/data, POST /purchase/airtime (NG)
│   │   ├── transactions.yaml          — GET /transactions
│   │   └── webhooks.yaml              — GET/POST /webhooks
│   └── components/
│       └── schemas.yaml               — shared request/response schemas + security scheme
└── guides/                            — human-readable walkthroughs, one per module
    ├── 01-getting-started.md
    ├── 02-authentication.md
    ├── 03-plans-and-networks.md
    ├── 04-wallet-funding.md
    ├── 05-purchasing-data-airtime.md
    ├── 06-transactions.md
    ├── 07-webhooks.md
    ├── 08-adding-a-country-rail.md    — start here before adding a new country
    └── 09-conventions-and-open-items.md
```

## How this was built

Captured directly from the live Swagger UI at
`https://telcos.opik.net/api/v1/docs` on 2026-09-06 (all ten operations
were expanded in the source capture, so parameters/request bodies/
response schemas below are transcribed from the real server, not
guessed). A few things — the exact auth header, the full error-response
shape, the full webhook event-name list — were **not** visible in that
capture (the Swagger "Authorize" modal and error-path examples weren't
open when the page was saved). Every place this spec had to infer
rather than transcribe is marked `CONFIRM` inline in the relevant
`.yaml`/`.md` file — see `guides/09-conventions-and-open-items.md` for
the consolidated list. Don't let these sit unconfirmed for long; they're
exactly the kind of gap that's cheap to close now and expensive to
discover in a client integration later.

## Updating this documentation

1. **A field or endpoint changes on an existing rail** → edit the one
   `paths/*.yaml` file (and `components/schemas.yaml` if the shape
   changed) for that module. Re-run the validation commands below.
2. **A new country's VTU rail is added** → read
   `guides/08-adding-a-country-rail.md` first. Don't retrofit the
   existing NG-only `Network` enum or the existing purchase operations —
   country rails get their own schema and their own operations so NG
   and (say) Ghana can evolve independently.
3. **The API key model changes** (see `handover.md` — public/secret key
   pair, Supabase-backed dashboard) → update
   `components/schemas.yaml#/securitySchemes` and
   `guides/02-authentication.md` together; they must never drift apart.

### Validating changes

```bash
cd docs/openapi
npx @redocly/cli lint openapi.yaml      # structural/style check
npx @redocly/cli bundle openapi.yaml    # confirms every $ref resolves
```

Both commands were run against this version of the spec before it was
committed — 0 errors, 1 cosmetic warning (missing `info.license`, not
applicable to a private API).
