# Stripe Full-Platform Discovery — Reference Model for B-Pay's Orchestration Layer

**Produced:** 2026-09-10, per direct product-owner instruction, extending
the existing "Stripe-as-Reference-Model Convention" (`handover.md`,
effective 2026-09-08) from "consult Stripe when a design blocker
appears" to a full, standalone discovery pass across Stripe's public
documentation, done once so every future task can cite a specific
section here instead of re-researching it.

**How to use this file:** it is a reference, not a task list. It
documents *how Stripe operates* — its object model, lifecycle
patterns, and operational conventions — captured from `docs.stripe.com`
and Stripe's own engineering writing. `handover.md`'s **Task 59**
is the task-facing counterpart: it takes every section below and turns
it into a gap (what B-Pay has vs. doesn't) and a proposed task. Read
this file for the "what Stripe does and why"; read Task 59 for "what
B-Pay should build because of it."

**Scope discipline, stated up front (same guardrail Task 57's own
Stripe-Reference section already states, repeated here because this
file is the largest single import of Stripe concepts this repo has
done):** B-Pay is a **ten-provider aggregation/orchestration layer**
sitting in front of African and international payment rails
(Korapay, Paystack, JuicyWay, Flutterwave, Remita, PaymentPoint,
`telcos.opik.net`, etc.) — it is not a payments processor holding its
own settlement balance, not a card network member, and does not move
money on rails of its own the way Stripe does. Every section below is
captured for its **pattern** (how to shape an API, a webhook, an error,
a ledger entry, a routing rule) so B-Pay can mirror the pattern at its
own scale — not as a list of Stripe features to blindly reproduce.
Where a Stripe concept has no B-Pay analogue (e.g. Stripe holding
funds and controlling settlement timing to a bank), this file says so
explicitly rather than forcing a mapping that doesn't exist.

---

## 1. The core object model — "intents" over imperative actions

Stripe's modern API (post-2019, what it now steers every new
integration toward) is built around **stateful intent objects**, not
one-shot imperative calls:

- **`PaymentIntent`** — created once per checkout/order attempt, then
  *confirmed* (possibly multiple times, e.g. after 3-D Secure
  authentication). It tracks the full lifecycle of a single payment
  attempt through statuses like `requires_payment_method` →
  `requires_confirmation` → `requires_action` → `processing` →
  `succeeded` / `canceled`, and "creates at most one successful
  charge." The intent, not the eventual charge, is the object a caller
  reasons about and stores a reference to.
- **`SetupIntent`** — the same pattern for *collecting* a payment
  method for later use with no immediate charge (saving a card on
  file).
- **`PaymentMethod`** — stateless, reusable payment-instrument data
  (card, bank account, mobile-money handle, etc.), typed by a `type`
  field with a matching nested hash (`type: "sepa_debit"` +
  `sepa_debit: {...}`). It is deliberately *not* transaction-specific
  (no amount/currency on it) — it gets attached to an intent or a
  Customer.
- **`Customer`** — the account-level identity a PaymentMethod is
  attached to for reuse; nothing is persisted to a Customer
  automatically, attaching is always an explicit step.
- The older, still-supported-but-discouraged **Charges API** is
  imperative and one-shot — Stripe's own guidance is "if starting a
  new integration, use PaymentIntents," precisely because the
  imperative model can't represent multi-step authentication or a
  cleanly retryable in-flight attempt.

**Why this matters for B-Pay:** B-Pay's own `/pay` and `/payout`
routes are already intent-shaped in spirit (Task 57's canonical
envelope + `provider_data`, and the `transactions` table recording
attempts) but are still fundamentally "one call, one provider response"
rather than a persisted, re-confirmable, multi-status object a caller
can poll or resume. See Task 59/b's gap table.

## 2. Idempotency — mandatory on every mutating call, not opt-in

- Every `POST` accepts an `Idempotency-Key` header (client-generated,
  e.g. a UUID or `customerId:orderId` composite). `GET`/`DELETE` don't
  need one — they're idempotent by definition.
- Stripe **caches the full result (status code + body) of the first
  request for a given key**, including failures, and replays that
  exact result for any retry with the same key — this is what makes it
  safe to blindly retry after a network error without risking a double
  charge.
- **Important caveat directly from Stripe's own advanced error-handling
  docs:** idempotency caching only kicks in *after* the request begins
  real execution. A `429` (rate limited) or a `401` (bad/missing key)
  can legitimately produce a *different* result on retry with the same
  key, because the rate limiter and auth layer run **before** the
  idempotency layer. Stripe's own recommendation: for most 4xx
  responses, generate a *fresh* idempotency key rather than reusing the
  old one when retrying a corrected request.
- Idempotency keys are how Stripe answers the exact question B-Pay's
  own `/pay`/`/payout` idempotency work (Task 12) had to answer for a
  single provider — but the pattern here is caller-supplied and
  provider-agnostic, not something each provider's own idempotency
  quirk has to be individually reverse-engineered (which is what B-Pay
  does today, one provider at a time).

## 3. Webhooks / Events — delivery, signing, retries, ordering

- Every webhook payload carries a `Stripe-Signature` header:
  `t=<unix timestamp>,v1=<HMAC-SHA256 hex digest>` computed over
  `"{timestamp}.{raw_request_body}"` with the endpoint's own signing
  secret. Verification **must** run against the *raw, unparsed* body —
  a framework that JSON-parses before the handler sees it is the
  single most common integration bug Stripe's own docs call out.
- **At-least-once delivery, not exactly-once.** The same `event.id` can
  arrive more than once (network retry, or Stripe's own retry after a
  non-2xx response) and events **can arrive out of order** relative to
  when they occurred (use the event's own `created` timestamp for
  ordering logic, never arrival order). Every consumer must be written
  to be idempotent on `event.id` — Stripe's own guidance is a `UNIQUE`
  DB constraint on a processed-events table, checked before any state
  mutation runs.
- **Retry schedule:** non-2xx (or slow, >~10s) responses trigger
  automatic retries with **exponential backoff for up to 3 days** in
  live mode; after 3 continuous days of failure Stripe **disables the
  endpoint** and notifies the account owner. The handler's own job is
  narrow and fast: verify signature → enqueue for async processing →
  return `2xx` immediately; slow work (emails, ledger posting, ERP
  sync) happens out-of-band, never inline in the handler.
- The Dashboard's own Events log shows every delivery attempt per
  endpoint and allows a manual re-send — i.e. Stripe treats "what
  happened to this specific webhook attempt" as a first-class,
  queryable record, not a fire-and-forget log line.

**Why this matters for B-Pay:** B-Pay's `webhookGateway.js` already
does per-provider signature verification (Paystack/Korapay/JuicyWay),
but there is **no persisted event log**, no replay/retry mechanism, and
no `event.id`-level idempotency guard — a duplicate webhook delivery
from any provider today re-runs whatever side effect the handler does,
with no de-dup layer. This is Task 59/c-1 below.

## 4. Errors and declines — a structured, layered taxonomy

Stripe separates **three distinct layers** of failure, each with its
own vocabulary, and never collapses them into one generic error:

1. **API-level errors** (`type`: `invalid_request_error`,
   `api_error`, `idempotency_error`, `rate_limit_error`,
   `authentication_error`, `card_error`, etc.) — HTTP-status-coded,
   raised before or independent of any attempt to move money.
2. **Card/processor declines** — Stripe's own normalized
   `decline_code` (e.g. `insufficient_funds`, `expired_card`,
   `fraudulent`, `generic_decline`, `lost_card`, `pickup_card`) sits on
   top of whatever raw code the card network/issuer returned, so a
   caller gets one consistent vocabulary regardless of which network
   handled the attempt. Stripe explicitly classifies each into **soft
   decline** (retryable — `insufficient_funds`,
   `generic_decline`) vs. **hard decline** (do not retry with the same
   instrument — `lost_card`, `fraudulent`, `stolen_card`,
   `security_violation`) — and documents this distinction precisely so
   integrators don't blindly retry a hard decline.
3. **`outcome` object on a charge** — a *third*, orthogonal signal:
   `network_decline_code`, `risk_level`, `risk_score`, `seller_message`,
   and `type` (`issuer_declined` vs. `blocked` — i.e. *whose* decision
   it was, the issuing bank's or Stripe's own Radar). This distinction
   matters operationally: a `blocked` outcome from Radar's own rule
   engine is not the same failure class as a bank decline and needs a
   different remediation (allow-list review, not "ask the customer to
   check their card").
4. **`fraudulent`** is explicitly called out as **never safe to
   retry** and **never safe to show verbatim to the end customer** —
   Stripe's own guidance is to display it identically to
   `generic_decline` so a would-be fraudster gets no signal about
   *why* they were blocked.

**Why this matters for B-Pay:** today, each provider file
(`providers/*.js`) does its own ad hoc error-message extraction (Task
45c fixed one specific instance of this for JuicyWay), and there is no
shared, cross-provider decline taxonomy — a `/pay` caller sees
whatever string shape each of the ten providers happens to return, not
one normalized `{ code, retryable, message }` shape. See Task 59/c-3.

## 5. Connect — multi-party money movement and platform fees

Stripe's Connect product is the closest existing Stripe concept to "one
platform routing money on behalf of many underlying accounts," which
is structurally close to what B-Pay already does across its ten
providers, so the *shape* of Connect's decisions is directly relevant
even though B-Pay is not literally running Connect.

- Three (legacy, still-relevant-for-pattern-purposes) connected-account
  types: **Standard** (fully self-managed by the connected party, full
  Stripe Dashboard), **Express** (Stripe-hosted onboarding + a
  simplified dashboard), **Custom** (platform owns the entire UX, no
  Stripe-provided dashboard at all). Newer integrations use a unified
  **Accounts v2** model (configurable `merchant`/`customer`/`recipient`
  roles + independent `dashboard: full|express|none` setting) instead
  of picking one of three fixed types — the underlying design lesson is
  the same either way: **how much of the account lifecycle the
  platform owns vs. delegates is a first-class, explicit setting**, not
  an afterthought.
- **Three charge-flow patterns**, each moving money differently:
  - **Direct charges** — charge lands on the connected account's own
    balance; the platform's balance only receives the
    `application_fee_amount` cut. Requires the connected account to
    hold its own processing capability.
  - **Destination charges** — charge lands on the *platform's* balance,
    then Stripe automatically transfers the destination amount to the
    connected account; refunding the original charge automatically
    reverses the transfer.
  - **Separate charges and transfers** — charge and transfer are two
    fully independent operations, letting one charge fan out to
    *multiple* connected accounts — this is Stripe's own answer to
    "split one payment across several recipients."
- **Platform monetization patterns**, all explicit, named
  mechanisms (not implicit math done by the caller): application fees
  on direct/destination charges, withholding an amount during
  transfer, one-off "account debits" against a connected account, or
  billing the connected account as a Customer of the platform itself
  for a recurring platform fee.
- **`connect_reserved` balance** — a platform-level pool explicitly
  used to *offset negative balances on connected accounts* (e.g. a
  refund/dispute that exceeds what a connected account currently
  holds) — i.e. Stripe models "who absorbs the loss when a downstream
  account can't cover a reversal" as a named, queryable balance, not an
  undocumented edge case.

**Why this matters for B-Pay:** B-Pay's own Task 53 (card issuance)
and Task 54 (KYC/KYB with a swappable default+fallback provider) are
already informally Connect-shaped — a `businesses` table (migration
`0010`) plus per-business, per-provider `api_keys` (migration `0012`)
is B-Pay's own version of "one platform, many connected accounts," but
there is no B-Pay analogue yet to Connect's charge-flow decision
(direct/destination/separate) for **B-Pay's own split-payment /
multi-recipient use cases** (e.g. a single collection that must fan out
to more than one downstream business or wallet) — that's a genuine
gap, not yet asked for by any confirmed product requirement. Flagged in
Task 59/c-6 as **speculative — do not build until a real B-Pay use
case names it**, per this file's own "pattern, not feature import"
scope discipline.

## 6. Balance, payouts, and reconciliation

- **`Balance`** object: `available` (can be paid out now) vs.
  `pending` (funds received but not yet settled — typically a
  rolling 2-day window, varies by country/account) — always broken out
  **per currency and per `source_types`** (card, bank_transfer, etc.),
  never a single undifferentiated number.
- **`BalanceTransaction`** — Stripe's own ledger entry, created for
  *every* event that moves the account balance (a charge, a refund, a
  payout, a fee, a dispute, an adjustment). This is the object
  Stripe's own reconciliation guidance is built around: don't
  reconstruct "what happened to my balance" from charges and payouts
  alone — read the balance-transaction stream, which is a complete,
  append-only audit trail with its own `type` taxonomy.
- **Payouts** — a `Payout` object transitions `pending` →
  `in_transit` → `paid` (or `failed`/`canceled`); payout *schedule*
  (daily/weekly/monthly/manual) is a completely separate, independently
  configurable setting from payout *speed* (how many business days
  after capture funds become eligible) — conflating the two is called
  out explicitly as a common misunderstanding in Stripe's own payouts
  FAQ.
- **2025-era Balance Settings API** (Stripe's own recent addition)
  formalizes per-currency **minimum-balance retention**
  (`minimum_balance_by_currency` — don't sweep everything on every
  payout, keep a buffer), **negative-balance recovery**
  (`debit_negative_balances`), and a configurable **settlement-timing**
  setting — i.e. Stripe itself is still actively productizing "how much
  of my own balance logic can I control" as a first-class API surface,
  not something bolted on once and left alone.

**Why this matters for B-Pay:** B-Pay currently has **no ledger at
all** — `transactions` (migration `0001`) records individual
attempts, but there is no append-only balance-transaction-style table,
no concept of "available vs. pending" for any wallet-style balance
B-Pay itself might hold (e.g. the `telcos.opik.net` per-business
`wallet` from Task 58 is opaque, queried live, not reconciled locally),
and no reconciliation job comparing B-Pay's own records against any
provider's statement/settlement report. This is very likely the single
largest structural gap relative to Stripe's model — see Task 59/c-2.

## 7. Disputes and refunds

- **`Refund`** objects reference the original charge; a charge can be
  partially or fully refunded, and (per Connect above) refunding a
  destination charge automatically reverses the associated transfer —
  Stripe keeps the money-movement graph consistent automatically
  rather than requiring the caller to manually undo each downstream
  effect.
- **`Dispute`** (chargeback) objects carry their own lifecycle
  (`needs_response` → `under_review` → `won`/`lost`/`warning_closed`
  etc.) and **automatically debit the relevant balance** (platform's
  own, or — per Connect — the connected account's, depending on charge
  type) for the disputed amount plus a dispute fee, immediately on
  dispute creation, before any resolution — i.e. Stripe treats a
  dispute as an immediate balance event, not something deferred until
  the dispute resolves.
- Refunds and disputes are surfaced through the same webhook/event
  system as payments (`charge.refunded`, `charge.dispute.created`,
  `charge.dispute.closed`, etc.) — there is no separate notification
  channel for "money came back out."

**Why this matters for B-Pay:** none of B-Pay's ten provider files
currently expose a unified refund or dispute/chargeback concept — each
provider's own refund/chargeback support (where it exists at all) is
undocumented in this repo today. This needs its own discovery pass per
provider before a unified `POST /refund` (Stripe-shaped) can be built
— flagged, not solved, in Task 59/c-4.

## 8. Fraud and risk — Radar

- Radar evaluates **every payment** against Stripe's own ML risk
  model plus any custom rules the account owner configures, and can
  **block a charge before it ever reaches the card issuer** — i.e.
  fraud screening happens *pre-authorization*, not as a post-hoc flag
  on an already-completed charge.
  the exact same `outcome.type: "blocked"` vs. `"issuer_declined"`
  distinction from Section 4 is how a caller tells "Radar's own call"
  apart from "the bank's call."
- Legitimate transactions Radar blocked can be manually allow-listed
  from the Dashboard — this does **not** retry the original payment
  automatically, it only clears the block for a *future* attempt.
- Risk scoring (`risk_level`, `risk_score`) is attached to the outcome
  of every charge regardless of whether Radar actually blocked
  anything, so a caller can build its own downstream policy (e.g.
  manual review above a threshold) on top of Stripe's score without
  waiting for an actual block.

**Why this matters for B-Pay:** B-Pay has **no fraud/risk layer of its
own** today — it passes every request straight through to whichever
provider its routing model selects, and inherits *that provider's*
fraud posture (or lack of one) with zero B-Pay-side signal of its own
(velocity checks, device/IP heuristics, an internal risk score). Given
B-Pay is explicitly a white-label platform for other businesses (Task
47's "fully hidden underlying rail" vision), this is a real gap once
B-Pay is signing up businesses it doesn't otherwise vet — flagged in
Task 59/c-7.

## 9. Identity verification / KYC

- Stripe Identity issues its own `VerificationSession` object
  (document capture, selfie/liveness check, database lookups),
  independent of any payment flow — it's consumed the same
  event/webhook-driven way as a payment (`identity.verification_session.verified`,
  `.requires_input`, etc.), not a synchronous blocking call.
- Connect's own account-onboarding flow (Section 5) layers KYB
  (business-level) verification on top of the same underlying
  primitive — collecting the *right* set of requirements per country/
  entity-type is itself a Stripe-maintained, versioned requirement set
  (`requirements.currently_due`, `requirements.eventually_due` on an
  Account object) rather than a hardcoded form.

**Why this matters for B-Pay:** Task 54 already made the real product
decision here (PaymentPoint default, swappable fallback, KYC/KYB
persisted in B-Pay's own DB rather than left live-only in the
provider) — this section's main contribution is the **event-driven,
not synchronous** pattern: B-Pay's own KYC status changes (`pending`→
`verified`→`rejected`) should be modeled and notified the same
webhook/event way as a payment status change, not as a special case.
No new task needed beyond what Task 54 already scoped; noted for
whichever session builds Task 54's implementation.

## 10. API keys and security

- **Secret keys** (`sk_live_...`/`sk_test_...`) can do *anything*;
  **Restricted API keys** (`rk_live_...`/`rk_test_...`) scope a key to
  an explicit, per-resource `Read`/`Write`/`None` permission set
  chosen at creation time — Stripe's own stated recommendation is to
  **default to restricted keys**, especially for any automated/agent
  caller, precisely so a leaked key's blast radius is bounded.
- **Sandbox (test mode) vs. live mode** are strictly partitioned —
  separate keys, separate object namespaces; a test-mode object can
  never be referenced from a live-mode call. This is the mechanism
  that makes end-to-end testing possible without any risk of touching
  real money, which is directly relevant to B-Pay's own currently-
  blocked Task 14 (no live testing possible from this sandbox at all,
  for any provider).

**Why this matters for B-Pay:** B-Pay's per-business, per-provider
`api_keys` table (migration `0012`, Task 58/c) already stores a single
key per `(business_id, provider)` pair with no scoping concept —
there is no equivalent of "this key can only read transactions" for,
say, a business's own downstream integrator. This is a real gap but a
low-priority one relative to Section 3/6's gaps — noted, not tasked,
in Task 59/d as a "future, not urgent" item.

## 11. Versioning, rate limits, pagination, metadata

- **API versioning** is date-named (`2025-09-30.clover`, etc.) and
  pinned per API key/request — a breaking change never silently
  applies to an existing integration; callers upgrade explicitly.
  B-Pay's own equivalent problem (a provider changing its API without
  warning) is exactly what caused several of this repo's own past
  incidents (Task 45a's wrong endpoint path, Korapay's self-
  contradicting docs in Task 53/a) — Stripe's answer is "the platform
  owns versioning," which isn't available to B-Pay as a mere caller of
  ten other platforms', but the *lesson* — pin and log which
  documented behavior a given provider integration was built against,
  so a future drift is detectable — is directly reusable.
- **Rate limiting** runs *before* the idempotency layer (Section 2) —
  a `429` is not idempotency-safe to blindly retry with the same key.
  B-Pay's own outbound calls to ten providers have no shared rate-limit
  awareness today; if any one provider starts `429`-ing under load
  there's no backoff/queueing layer to absorb it.
- **Expandable objects / `expand[]`** and **cursor-based pagination**
  are Stripe's answer to "don't force the caller to make N follow-up
  requests to hydrate a list" — relevant mainly to B-Pay's own
  eventual admin dashboard (Task 46) once it lists transactions/
  businesses/api-keys at scale.
- **`metadata`** — every object accepts an arbitrary caller-defined
  key-value bag, explicitly *not* used by Stripe's own logic — this is
  the documented, sanctioned way to attach a caller's own reference IDs
  (order number, internal customer ID) without polluting typed fields.
  B-Pay's own envelope (Task 57) has no equivalent free-form field
  today.

## 12. Multiprocessor Orchestration — Stripe's own answer to "B-Pay's exact problem"

This is the single most directly relevant Stripe product to B-Pay's
own purpose, and is captured in its own section rather than folded
into Section 1, because it is literally Stripe (as a platform)
solving "route a payment across multiple underlying processors" — the
same problem B-Pay solves across Korapay/Paystack/JuicyWay/Flutterwave/
etc. (Currently in private preview at Stripe; captured here for its
*design*, not as a claim that B-Pay should integrate with Stripe's own
Orchestration product.)

- **Declarative, ordered rules**, each an explicit `condition` (card
  country, currency, amount, etc.) + `action` (route to processor X),
  evaluated **left to right, first match wins**, with an explicit
  default action for anything that matches no rule. This is
  structurally the same shape as B-Pay's own Task 51 routing tables
  (international vs. African domain, default + numbered fallbacks) —
  Stripe's version is per-condition/rule-based rather than per-domain,
  which is a strictly more general version of the same idea.
- **Cross-processor retries ("waterfall")** — a payment that fails on
  its first-chosen processor can be automatically retried on a
  *different* processor within the same logical attempt, not just
  logged as failed. B-Pay's Task 51 model has "fallback" providers
  *listed* per domain, but **`routes.js` does not actually retry a
  failed request against a fallback provider today** — a failure on
  the domain default is returned to the caller as a failure, full
  stop. This is a concrete, nameable gap.
- **Per-processor performance analytics** (auth/acceptance rate
  benchmarked per processor, filterable by currency/card
  country/type) — the operational visibility layer that makes
  "which processor should be default" an evidence-based, revisitable
  decision instead of a one-time manual call (which is how Task 51's
  own default/fallback assignments were made — as documented product-
  owner decisions, not measured outcomes).
- **Unified refunds across processors** — a refund can be issued
  through the orchestrating platform even when a *different* processor
  handled the original charge, i.e. the caller-facing refund API
  doesn't leak which processor happens to be involved.

**This section is the clearest confirmation that B-Pay's Task 51
routing model is the right shape** (default + fallback, evaluated per
request) but is **incomplete relative to what Stripe itself considers
"orchestration"**: no automatic cross-provider retry/waterfall, no
per-provider performance tracking, and no unified refund path. These
three gaps are Task 59/c-5's direct citation of this section.

---

## Summary table — every section, one line each

| # | Stripe concept | B-Pay's closest existing equivalent | Verdict |
|---|---|---|---|
| 1 | PaymentIntent/PaymentMethod/Customer lifecycle | `/pay`+`/payout` + `transactions` + Customer Vault (Task 57) | Partial — no persisted multi-status intent object |
| 2 | Idempotency keys (caller-supplied, universal) | Task 12 (provider-specific idempotency) | Partial — not a universal, caller-supplied mechanism |
| 3 | Webhooks: signing, retries, event log, dedup | `webhookGateway.js` (signing only) | **Gap** — no event log, no dedup, no replay |
| 4 | Layered error/decline taxonomy | Ad hoc, per-provider (Task 45c fixed one instance) | **Gap** — no shared taxonomy |
| 5 | Connect (multi-party charge flows) | `businesses`/`api_keys` (Task 58/c) | Speculative — no confirmed B-Pay split-payment need yet |
| 6 | Balance / BalanceTransaction ledger | None | **Gap** — largest structural gap |
| 7 | Refunds / Disputes | None | **Gap** — not even discovered per-provider yet |
| 8 | Radar (fraud/risk) | None | **Gap** — no B-Pay-side risk layer |
| 9 | Identity/KYC (event-driven) | Task 54 (decided, not yet built) | On track — no new task needed |
| 10 | Restricted API keys | `api_keys` table, one key per (business, provider), unscoped | Minor gap, low priority |
| 11 | Versioning / rate limits / pagination / metadata | None formalized | Minor gap, mostly relevant to Task 46's dashboard |
| 12 | Multiprocessor Orchestration (rules/waterfall/analytics/unified refunds) | Task 51 routing tables (default+fallback, no retry) | **Gap** — closest 1:1 match to B-Pay's own purpose |

**Sources consulted (all `docs.stripe.com` unless noted), 2026-09-10:**
`/payments/payment-methods`, `/api/payment_intents`, `/api/payment_methods`,
`/payments/payment-methods/transitioning`, `/api/idempotent_requests`,
`/plan-integration/get-started/server-side-integration`, `stripe.com/blog/idempotency`,
`/webhooks`, `/webhooks/signature`, `/declines`, `/declines/codes`,
`/declines/network-codes`, `/connect/charges`, `/connect/collecting-fees`,
`/connect/account-balances`, `/api/balance`, `/api/balance/balance_object`,
`/api/payouts`, `support.stripe.com/questions/payout-schedules-faq`,
`support.stripe.com/questions/payouts-faq`,
`/changelog/clover/2025-09-30/balance-settings-ga`, `/keys`,
`/keys/restricted-api-keys`, `/apis`, `/error-low-level`,
`/payments/orchestration/rules`, `/payments/orchestration/route-payments`,
`/payments/orchestration/retries`, `stripe.com/guides/evaluating-and-managing-multiple-payment-providers`,
plus secondary analysis (FlyCode, Svix, Hookdeck, brandur.org, apideck.com)
cross-checked against the primary docs above rather than relied on alone.
