# Conventions and open items (audit)

This file consolidates every place this documentation set had to infer
rather than transcribe directly from the live server, plus the
conventions that were confirmed. Nothing here should be treated as
"probably fine" — each unconfirmed item is a real gap between what the
Swagger UI capture showed and what a client integration needs to know
for certain.

## How this audit was done

Source: an MHTML snapshot of `https://telcos.opik.net/api/v1/docs`
(Swagger UI), saved 2026-09-06, with all 10 operations already
expanded (`is-open`) at save time — so parameters, request bodies, and
`200` response schemas were transcribed directly from rendered DOM,
not guessed. The snapshot was parsed programmatically (BeautifulSoup)
rather than read visually, to avoid transcription errors on schema
field names/types. Every field in `docs/openapi/` traces back to a
specific block in that capture.

## Confirmed

- Base URL: `https://telco.opik.net/api/v1` (production, only server
  listed).
- Title/version: "Reseller API" v1.0.0, OAS 3.0.
- All 10 operations, their exact paths, HTTP methods, summaries, and
  `200` response schemas (see module guides 02–07 for each).
- Query parameters and defaults for `GET /plans` (`network`,
  `category`) and `GET /transactions` (`limit=20`, `offset=0`).
- The `{ success, data }` response envelope, consistent across all 10
  operations' `200` responses.
- An API-key-based auth scheme exists (an "Authorize" control is
  present, named `api_key`).

## Unconfirmed — CONFIRM before relying on these

| # | Item | Where flagged | Why it matters |
|---|------|----------------|-----------------|
| 1 | Exact auth header/format (`Authorization: Bearer`, raw `x-api-key`, or other) | `components/schemas.yaml#/securitySchemes`, `guides/02-authentication.md` | Every authenticated call is unimplementable correctly without this |
| 2 | Full error-response shape | `components/schemas.yaml#/schemas/ErrorResponse` | No 4xx/5xx example was in the capture (only `200`s were expanded); the documented shape is a placeholder borrowed from this repo's own `B-Pay-backend` convention, not observed on this server |
| 3 | Full `transactions[].type` enum (only `"deposit"` observed) | `components/schemas.yaml#/schemas/TransactionType`, `guides/06-transactions.md` | Client code branching on transaction type will miss cases |
| 4 | Full `transactions[].status` enum (only `"pending"` observed) | `components/schemas.yaml#/schemas/TransactionStatus`, `guides/06-transactions.md` | Same risk — status-based logic (e.g. "is this purchase done?") can't be written safely yet |
| 5 | Full `webhooks[].events` value set | `components/schemas.yaml#/schemas/Webhook`, `guides/07-webhooks.md` | A business can't build event-routing logic against an unknown set |
| 6 | Webhook signing scheme (HMAC? which header carries the signature?) | `guides/07-webhooks.md` | Needed to verify a webhook is really from this platform, not spoofed. **Re-checked 2026-09-09** against a live screenshot of the `/webhooks` Swagger section — matches this repo's existing capture exactly; that page is the registration CRUD API and was never going to show delivery-time signing, so it's ruled out as a source, not resolved. Still needs a delivered payload's headers, a dedicated security doc, or direct confirmation. |
| 7 | Whether `webhooks[].secret` is client-supplied or server-generated | `guides/07-webhooks.md` | Changes both the request contract and the security posture. `secret` appears as a plain request-body field on `POST /webhooks` (re-confirmed via the same 2026-09-09 screenshot) — supports the client-supplied reading, but still not a direct statement from the provider either way |
| 8 | Insufficient-balance behavior on `POST /purchase/*` | `paths/purchases.yaml` (marked `402`, unconfirmed) | Needed for correct client-side error handling before money is involved |
| 9 | Whether `GET /plans` will need a `country` filter or a separate endpoint once a second country ships | `guides/08-adding-a-country-rail.md` | Determines the shape of the very next change to this spec |
| 10 | Deposit settlement signal — polling `/wallet` vs. a dedicated webhook event | `guides/04-wallet-funding.md` | Affects whether businesses need to poll or can rely purely on webhooks |

## Conventions this documentation set itself follows

- One file per module (paths, guides) — see `docs/README.md`.
- Every schema field traceable to either an observed example or an
  explicit `CONFIRM` note — never a silent guess.
- New countries get new schemas/operations, never a parameter bolted
  onto the NG-only ones (`08-adding-a-country-rail.md`).
- This audit table is the single place to check "is X confirmed yet?"
  — when an item above gets confirmed, delete its row here **and**
  remove the corresponding `CONFIRM`/description note in the spec file
  itself, so the two never disagree about what's still open.
