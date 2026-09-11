# Adding a country rail

The current live rail is Nigeria (NG): four networks (MTN, Airtel, Glo,
9mobile), one currency (NGN), one plans catalog. When a second country
is added, resist the temptation to bolt it onto the existing NG
schema/enum with an `if country === 'GH'` branch buried somewhere —
that's how a "modular, always-updatable" spec quietly turns back into
one big tangled one. Instead, give the new country its own pieces at
every layer:

## Checklist

1. **Network enum** — add a new schema, not new values to the existing
   one.
   ```yaml
   # components/schemas.yaml
   NetworkGH:
     type: string
     description: Ghana mobile network operators.
     enum: [MTN_GH, VODAFONE_GH, AIRTELTIGO]
   ```

2. **Purchase operations** — add a new operation pair, not a `country`
   parameter on the existing NG ones. Different countries can have
   different required fields (e.g. some data plans need a bundle
   duration, some airtime rails need a recipient-network validation
   step NG doesn't) — separate operations let each evolve without a
   shared shape holding both back.
   ```yaml
   # paths/purchases.yaml
   purchaseDataGH:
     post:
       tags: [Purchases]
       operationId: purchaseDataGH
       summary: Purchase a data bundle (Ghana)
       # ...same shape as purchaseData, using NetworkGH
   ```
   Then wire it into `openapi.yaml`'s `paths:` map, e.g. under
   `/gh/purchase/data` (see point 4 on URL shape).

3. **Plans catalog** — confirm with the provider whether `GET /plans`
   returns all countries' plans together (filterable by a new `country`
   query param) or whether each country has its own plans endpoint.
   Don't assume; this determines whether `plans.yaml` needs a `country`
   parameter added or a sibling operation. Mark whichever you didn't
   confirm as `CONFIRM` in the file, same convention as the rest of
   this spec.

4. **URL/path shape** — decide once, up front, and apply consistently:
   either
   - a country prefix in the path (`/ng/purchase/data`,
     `/gh/purchase/data`), or
   - a separate `server` entry per country in `openapi.yaml`'s
     `servers:` list with identical paths on each
     (`https://gh.telco.opik.net/api/v1`).

   Whichever is chosen, retrofit the *existing* NG paths to match
   rather than leaving NG unprefixed as a silent special case — a
   client library generated from this spec should not need an
   if-NG-then-else to call the right URL.

5. **Currency** — every amount field (`price`, `amount`) is currently
   unitless/assumed NGN. Once a second currency exists, add an explicit
   `currency` field (ISO 4217, e.g. `"NGN"`, `"GHS"`) to `Plan`,
   `PurchaseResponse`, and `Transaction` in `components/schemas.yaml` —
   don't leave amounts ambiguous across countries.

6. **Webhook event payloads** — confirm whether event `type` values
   gain a country suffix (`purchase.success.gh`) or whether `metadata`
   carries the country instead. Document the decision in
   `guides/07-webhooks.md`, not just in code.

7. **Docs** — add `guides/05b-purchasing-data-airtime-gh.md` (or
   whichever ISO2 code) alongside the NG guide rather than editing the
   NG guide to cover both — same one-module-per-file reasoning as the
   spec itself.

8. **Validate** — run `npx @redocly/cli lint openapi.yaml` and
   `bundle` after wiring in the new paths/schemas, same as any other
   change (see `docs/README.md`).

## What NOT to do

- Don't add a bare `country` string field to the existing NG
  `Network` enum's purchase requests "to save a file" — it defeats the
  entire point of making this modular in the first place.
- Don't guess a new country's network list, plan shape, or currency
  from general knowledge of that country's telecom market — confirm
  it against that country's actual provider/aggregator contract, the
  same rigor `handover.md` already insists on for every new payment
  provider in the sibling `B-Pay-backend` codebase.
