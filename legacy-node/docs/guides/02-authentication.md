# Authentication

## Today

`POST /auth/register` and `POST /auth/login` both return a single
`api_key` string per business/user:

```json
{ "success": true, "data": { "id": "...", "email": "...", "api_key": "..." } }
```

**Confirmed (2026-09-09)**, directly against the live Swagger UI's
"Available authorizations" modal: the header is a raw `X-API-Key`,
no `Bearer` prefix —

```
X-API-Key: <api_key>
```

e.g. `X-API-Key: sk_live_xxxxxxxxxxxxxxxx`. The example value in the
modal itself uses an `sk_live_` prefix, which was not previously
documented here — worth noting since the "Today" section above
describes `api_key` as a single opaque string; in practice the value
already looks like a secret key in the Stripe/Paystack sense, not a
prefix-less token. `components/schemas.yaml#/securitySchemes/apiKeyAuth`
is updated to match.

## Planned (see `handover.md`, "Business dashboard & Supabase
integration")

The single `api_key` is planned to become a **public/secret key pair**,
industry-standard style:

- `pk_live_...` / `pk_test_...` — public key, safe to embed in
  client-side code, identifies the business.
- `sk_live_...` / `sk_test_...` — secret key, server-side only, used
  for authenticated calls and never shown again after initial issuance
  (same convention Stripe/Paystack/Korapay use).

Businesses will generate/rotate/revoke these from the new dashboard
rather than only receiving one at registration. This changes
`AuthUser` and `securitySchemes` in `components/schemas.yaml` — update
both together with this file when that work starts, per
`docs/README.md`'s update instructions.
