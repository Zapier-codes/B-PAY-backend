# B-Pay-backend — Error Taxonomy (Task 62)

**Status: internal shape, confirmed and wired; NOT yet exposed to
external callers.** Read the "What callers actually get today"
section before assuming any of the fields below show up in an HTTP
response — they don't yet. This document exists so that (a) whoever
does Task 62/e (per-provider retrofit) knows the target shape without
re-deriving it, and (b) whoever eventually wires this into the actual
route responses has one page to update instead of guessing.

Full design history and reasoning lives in `handover.md`'s Task 62
section (parts a–c) — this page is the reference summary, not a
replacement for that record.

## The shape

Every failure that passes through `handleApiCall()` /
`providerError()` in `utils/helpers.js` is thrown as an `ApiError`
carrying:

| Field | Type | Meaning |
|---|---|---|
| `type` | enum, 8 values (below) | Broad failure category, mirrors Stripe's own error-type taxonomy |
| `code` | string \| `null` | Specific failure reason, e.g. `insufficient_funds`, `invalid_account_number` |
| `decline_code` | string \| `null` | Network/issuer-level reason underneath `code: card_error`, where a provider exposes a two-tier signal (rare for B-Pay's own providers — expect `null` far more often than Stripe would) |
| `param` | string \| `null` | The specific request field the failure relates to, when known |
| `message` | string | Safe-to-surface text (unchanged from today's existing behavior) |
| `transaction_id` | string \| `null` | B-Pay's own transaction reference, when the failure can be tied to one |
| `doc_url` | string \| `null` | Always `null` today — see "What's still open" below |
| `retryable` | boolean (getter, not stored) | Derived from `type` via `isRetryable(type)` |

### `type` — the 8 values

| Value | Meaning |
|---|---|
| `api_error` | Unmapped/unrecognized provider failure shape (today's default — see below) |
| `card_error` | A card was declined for a reason the provider actually communicated |
| `idempotency_error` | A reused idempotency key/reference against different parameters |
| `invalid_request_error` | Bad/missing request parameters |
| `authentication_error` | Bad/expired/missing provider API key |
| `rate_limit_error` | Provider's own 429 |
| `permission_error` | Valid key, insufficient permission for the action |
| `api_connection_error` | Network failure/timeout talking to the provider |

`retryable` is `true` only for `api_error`, `api_connection_error`,
and `rate_limit_error` — everything else is treated as not
worth retrying against the same input.

## Per-provider status (Task 62/b)

- **Korapay, Paystack, Flutterwave, TelcosOpik** — read only a flat
  `responseData.message` string today. Every failure becomes
  `type: api_error` by default; none of these four populate `code`.
- **JuicyWay** — the one provider whose real error envelope
  (`{ error: { code, message, type, details } }`) already contains
  `code`/`type`, but the current code only reads `.error?.message`
  and discards the rest. Lowest-risk starting point for Task 62/e
  once picked up, since no new provider-side research is needed.
- **Xixapay, PaymentPoint, Prestmit, DodoPayments, Remita** — no
  provider file exists yet for any of these; not retrofit candidates
  until their own provider files are built.

## What callers actually get today — the real gap

**`type`/`code`/`decline_code`/`param`/`transaction_id`/`doc_url`/
`retryable` are not present in any HTTP response body today.**
Every route's `catch` block in `routes.js` still does exactly what it
did before Task 62 started:

```js
return res.status(error.statusCode || 500).json({
  status: false,
  message: clientSafeMessage(error, 'Some fallback message'),
});
```

`clientSafeMessage()` only ever returns a string. The new fields live
on the `ApiError` instance that's thrown, but nothing in `routes.js`
reads them back off before building the JSON response — so today the
new taxonomy is real internal plumbing (useful for server-side
logging and for Task 63's future automatic-fallback logic, which
needs `retryable` at the point of use) but callers of `/pay`,
`/payout`, and the VTU routes see exactly the same
`{ status: false, message }` shape they always have.

**Exposing these fields to callers is a separate, not-yet-decided
change**, for the same reason Task 62/a's HTTP-status-alignment table
was deliberately left unapplied: changing what a live API response
contains is a caller-facing contract change, and this repo's own
discipline treats that as something that needs its own explicit
product-owner go-ahead rather than shipping as a side effect of
documentation or internal wiring. Not built here — flagged for
whoever picks it up next (candidate follow-on, not yet numbered as
its own task: add `type`/`code`/`decline_code`/`param`/
`transaction_id`/`retryable` to each route's error-response `json()`
call, once someone actually confirms callers should receive them and
in what wire shape — flat on the response body, or nested under an
`error` key the way Stripe itself nests its object).

## `doc_url`

Stays `null` everywhere until the field above is actually resolved —
`doc_url` is meant to point a caller at the specific `code`'s
explanation, which only matters once `code` itself reaches a caller.
Once that happens, this file is the intended anchor target (e.g.
`doc_url: ".../ERROR_HANDLING.md#the-8-values"` style), not a new
document to be created from scratch at that point.
