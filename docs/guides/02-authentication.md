# Authentication

## Today

`POST /auth/register` and `POST /auth/login` both return a single
`api_key` string per business/user:

```json
{ "success": true, "data": { "id": "...", "email": "...", "api_key": "..." } }
```

**Unconfirmed — flagged, not guessed away:** the live Swagger UI shows
a lock icon and an "Authorize" control (security scheme name `api_key`),
but the modal that states the exact header/format wasn't open in the
capture this doc is built from. Before writing client code against
this, confirm whether it's:
- `Authorization: Bearer <api_key>`, or
- a raw `x-api-key: <api_key>` header, or
- something else entirely.

`components/schemas.yaml#/securitySchemes/apiKeyAuth` currently assumes
the `Authorization` header as the most common industry pattern — treat
that as a best guess, not a confirmed fact, until checked against the
live server or its source.

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
