import express from 'express';
import { Paystack } from './providers/paystack.js';
import { Juicyway } from './providers/juicyway.js';
import { Korapay } from './providers/korapay.js';
import { TelcosOpik, provisionTelcosOpikAccount, resolveTelcosOpikApiKey } from './providers/telcosOpik.js';
import { log, formatPayload, generateReference, getSupportedCurrencies, isValidCurrencyCode, isValidEmail, providerRequiresEmail, requireInternalApiKey, classifyDomain, computeProviderEventKey } from './utils/helpers.js';
import { recordTransaction, recordBalanceTransaction, getBusinessBalance, getTransactionByReference, getRoutingDefaultProvider, getCapabilityStatus, isWebhookEventProcessed, recordWebhookEvent, markWebhookEventStatus, getWebhookEventById, getRecentWebhookOutcomes } from './utils/supabase.js';
import { getMissingFields } from './utils/fieldRequirements.js';
import { resolveCustomer } from './utils/customerVault.js';
import { handleGatewayEvent } from './webhookGateway.js';
import { notifyOps } from './utils/alerts.js';

const router = express.Router();

// ==================================================
// 🧠 SMART ROUTING CONFIGURATION
// ==================================================
// 💸 PAYOUT / DISBURSEMENT (Task: B-Pay-backend payout flow)
// ==================================================
// Outbound money movement — paying listeners, refunds, settlements.
// Routed to Korapay via ROUTING_RULES.payout above.
//
// Body shape:
//   {
//     "amount": 5000,
//     "currency": "NGN",
//     "bank_code": "057",        // Zenith Bank, from getBanks()
//     "account_number": "1234567890",  // Nova Bank "tag" = account_number
//     "narration": "Listener earnings — Aug 2026 cycle",
//     "reference": "optional-custom-ref",
//     "customer": { "name": "John Doe", "email": "john@example.com" }
//   }
router.post('/payout', requireInternalApiKey, async (req, res) => {
  try {
    const { amount, currency, bank_code, account_number, narration, reference, customer, payment_method } = req.body;

    assertValidAmount(amount);
    assertValidCurrencyFormat(currency);

    if (!bank_code || typeof bank_code !== 'string') {
      const err = new Error(`'bank_code' is required and must be a non-empty string (received: ${JSON.stringify(bank_code)})`);
      err.statusCode = 400;
      throw err;
    }

    if (!account_number || typeof account_number !== 'string') {
      const err = new Error(`'account_number' is required and must be a non-empty string (received: ${JSON.stringify(account_number)})`);
      err.statusCode = 400;
      throw err;
    }

    // Task 52/e-2, part b-i (2026-09-08): same domain-aware precedence
    // as POST /pay (e-2a) — explicit `provider` wins (unchanged,
    // already satisfies "explicit fallback request"), otherwise
    // classifyDomain(currency) -> DOMAIN_DEFAULT_PROVIDER (defined
    // further down this file, shared with /pay). Replaces the old
    // flat ROUTING_RULES.payout ('korapay' always) default, which was
    // wrong under Task 51's model for an international payout (should
    // default to Juicyway, not Korapay). Flagging plainly: this is an
    // intentional behavior change for any caller that omitted
    // `provider` and expected Korapay regardless of currency.
    let providerName = req.body.provider;
    if (!providerName) {
      const domain = classifyDomain(currency);
      providerName = await resolveDomainDefaultProvider(domain);
    }
    const provider = getProvider(providerName);

    assertCurrencySupported(providerName, currency);

    // Task 56/d-3-c: this route (unlike POST /pay) has never computed
    // its own `reference` up front — it just forwards the caller's
    // `reference` (possibly undefined) straight into
    // provider.processPayout(), which falls back to its OWN internally
    // generated one (e.g. Korapay's processPayout(): `data.reference ||
    // generateReference('korapay-payout')`) when omitted. That
    // provider-generated value was never returned to this scope before
    // now, so recordTransaction() below would have logged the wrong
    // reference (or none) for any caller that omitted one — silently
    // breaking Task 56/d-4's future by-reference lookup for exactly
    // those payouts. Fixed here by computing the reference in this
    // handler up front, the same way POST /pay already does (`ref =
    // reference || generateReference(providerName)`), and forwarding
    // that explicit value into processPayout() instead of leaving the
    // provider to generate its own. Flagging plainly: this changes the
    // auto-generated reference's prefix for a caller that omits
    // `reference` (was e.g. `KORAPAY-PAYOUT-...` from inside the
    // provider, is now `KORAPAY-...` — matching /pay's own convention)
    // — a caller relying on the old prefix specifically would see a
    // different (still valid, still unique) format.
    const payoutRef = reference || generateReference(providerName);

    const result = await provider.processPayout({
      amount,
      currency,
      bank_code,
      account_number,
      narration,
      reference: payoutRef,
      customer,
      payment_method,
    });

    log(`Payout success via ${providerName}: ${formatPayload(result)}`);

    // Task 56/d-3-c: best-effort transaction record, same
    // fire-and-forget pattern as POST /pay's own d-3-b — not awaited,
    // so a slow/unreachable Supabase insert can never delay this
    // route's response; recordTransaction() (d-3-a) already never
    // throws, so there's nothing to `.catch()` here either.
    //
    // status: 'pending', same reasoning as d-3-b — Korapay's own
    // processPayout() resolving confirms only that the disbursement
    // request was *accepted*, explicitly NOT that the transfer
    // completed (see providers/korapay.js's own extensive comment on
    // this above `return result` — Kora's real lifecycle state lives
    // in `data.status: 'processing'`, confirmed later via webhook or
    // GET /payout/verify). Using the same 'pending' value POST /pay
    // uses keeps one consistent meaning for the column across both
    // write sites, rather than inventing a payout-specific status
    // value not in migration 0001's own CHECK constraint list
    // ('pending' | 'success' | 'failed').
    recordTransaction({
      reference: payoutRef,
      type: 'payout',
      provider: providerName,
      currency,
      amount,
      status: 'pending',
    });

    // Task 61/b: best-effort ledger record, same fire-and-forget
    // posture as recordTransaction() immediately above — see that
    // helper's own header comment (utils/supabase.js) for why
    // `business_id`/`transaction_id` are both left unset here.
    // `type: 'payout'` matches the balance_transactions CHECK
    // constraint's own value for this flow (migration 0014) directly.
    recordBalanceTransaction({
      reference: payoutRef,
      provider: providerName,
      type: 'payout',
      amount,
      currency,
    });

    res.json({ status: 'success', data: result });
  } catch (error) {
    log(`Payout error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Payout failed'),
    });
  }
});

// GET /api/payout/verify?reference=XYZ&provider=korapay
// Task 42 Part ii (handover.md): wires verifyPayout() (built, not yet
// reachable by anything outside this repo) into an actual route —
// closes half of the "no way to learn a processing payout's true
// final outcome" gap; the webhook-handler half remains separately
// open. GET + query params, mirroring /verify's own shape exactly
// (this is the payout-side counterpart to that collection-side
// route) — not POST, since this only reads a transaction's state,
// never changes anything.
//
// requireInternalApiKey, same as /payout itself: this reveals payout
// destination/amount/status details, the same sensitivity class as
// initiating one — no reason for a weaker gate on the read side than
// the write side already has.
//
// providerName defaults to ROUTING_RULES.payout (korapay), matching
// /payout's own default-provider pattern exactly, NOT hardcoded to
// 'korapay' directly — if a future provider is ever added as more
// than a stub (Task 43's own architecture note), this route doesn't
// need to change to support it, same as /payout doesn't.
//
// Guards against calling a nonexistent method on a provider that
// hasn't implemented payout verification yet (today, everything
// except Korapay — Paystack/JuicyWay per Task 43's own
// "stub until fully integrated" rule) with a clear 501, rather than
// letting `provider.verifyPayout is not a function` reach the client
// as an unhandled crash.
//
// Task 56/d-4: this route has no `currency` of its own to classify —
// it only ever received `reference` (+ optional explicit `provider`).
// Per Task 56/a's product-owner decision, the query-param-fallback
// option is explicitly dropped; instead, when no explicit `provider`
// is given, this looks the original payout's `currency` up from the
// `transactions` table (written by `POST /payout`, Task 56/d-3-c) by
// `reference`, and routes off THAT via the same
// classifyDomain()/DOMAIN_DEFAULT_PROVIDER model `POST /pay` and
// `GET /banks` already use. A lookup miss — no matching row, the
// original write failed, Supabase unreachable, or the payout
// predates this table — falls straight through to today's existing
// `ROUTING_RULES.payout` (Korapay) default, per (a)'s explicitly
// accepted trade-off. `getTransactionByReference()` (d-4's own read
// helper) never throws, so this never turns a lookup miss into a 500.
router.get('/payout/verify', requireInternalApiKey, async (req, res) => {
  try {
    const { reference, provider: providerParam } = req.query;

    if (!reference || typeof reference !== 'string') {
      const err = new Error(`'reference' query param is required and must be a non-empty string (received: ${JSON.stringify(reference)})`);
      err.statusCode = 400;
      throw err;
    }

    let providerName = providerParam;
    if (!providerName) {
      const transaction = await getTransactionByReference(reference);
      if (transaction && transaction.currency) {
        const domain = classifyDomain(transaction.currency);
        providerName = await resolveDomainDefaultProvider(domain);
      } else {
        // Miss — see this route's own comment above and Task 56/a's
        // accepted trade-off: fall through to today's existing
        // default rather than failing the request.
        providerName = ROUTING_RULES.payout;
      }
    }

    const provider = getProvider(providerName);

    if (typeof provider.verifyPayout !== 'function') {
      const err = new Error(`Payout verification is not implemented for provider '${providerName}'`);
      err.statusCode = 501;
      throw err;
    }

    const result = await provider.verifyPayout(reference);

    log(`Payout verification success via ${providerName}: ${formatPayload(result)}`);
    res.json({ status: 'success', provider: providerName, data: result });
  } catch (error) {
    log(`Payout verification error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Payout verification failed'),
    });
  }
});

// GET /banks — returns a bank/institution list for a currency.
// Query: ?currency=NGN (defaults to NGN), ?provider=korapay (optional
// explicit override)
//
// Task 52/e-2, part c (2026-09-08): same explicit-provider-wins-then-
// domain-default precedence as e-2a/e-2b-i. Previously this route
// unconditionally called Korapay regardless of currency, with no way
// for a caller to request a different provider at all — under Task
// 51's model an international-currency bank-list lookup should
// default to Juicyway, and there was no explicit-fallback mechanism
// here to begin with (unlike /pay and /payout, which already had
// req.body.provider). Both gaps close together here since they're the
// same fix. Flagged plainly: intentional behavior change — a caller
// relying on this route always hitting Korapay regardless of currency
// now gets routed by currency instead (still Korapay for African-rails
// currencies, the common case, so most existing callers are
// unaffected; only a caller passing a non-African-rails `currency`
// changes behavior).
router.get('/banks', async (req, res) => {
  try {
    const currency = (req.query.currency || 'NGN').toString().toUpperCase();
    assertValidCurrencyFormat(currency);

    let providerName = req.query.provider;
    if (!providerName) {
      const domain = classifyDomain(currency);
      providerName = await resolveDomainDefaultProvider(domain);
    }

    const provider = getProvider(providerName);
    const result = await provider.getBanks(currency);

    res.json({ status: 'success', provider: providerName, data: result });
  } catch (error) {
    log(`Banks list error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Failed to fetch bank list'),
    });
  }
});

// ==================================================

const ROUTING_RULES = {
  collect_payment: 'paystack',
  bank_transfer: 'korapay',
  payout: 'korapay',
  international: 'juicyway',
};

// Task 52/e-2, part a (2026-09-08) — Task 51's collection-domain
// defaults, keyed by classifyDomain()'s two possible return values.
// Deliberately its own small table, not folded into ROUTING_RULES
// above: ROUTING_RULES is the OLD action-string model Task 51 is
// superseding; ROUTING_RULES itself is left untouched here since
// /payout and /banks (Task 52/e-2 parts b/c, not done this session)
// still read it directly. Only POST /pay's own provider-resolution
// logic below has been switched over to this table.
const DOMAIN_DEFAULT_PROVIDER = {
  african_rails: 'korapay',
  international: 'juicyway',
};

// Task 52/e-2d (2026-09-08) — resolves this leaf's own open design
// question (env var vs. config file vs. admin-dashboard toggle) via
// the Stripe-precedent option the product owner directed this task to
// mirror: Payment Method Configurations are a live, Dashboard-
// toggleable, API-backed object, not a deploy. `routing_config`
// (migrations 0005/0006) is that object's equivalent here — reads a
// domain's default provider from Supabase first, so a promotion
// ("make Juicyway the default for african_rails instead of Korapay")
// takes effect on the next request with no code deploy, exactly the
// property env-var/config-file approaches both fail per the Stripe
// writeup in handover.md's Task 52/e-2d entry.
//
// `DOMAIN_DEFAULT_PROVIDER` above is NOT deleted — it's kept as this
// function's own fallback/safety-net, seeded with the exact same
// values migration 0005 seeds `routing_config` with. A Supabase miss
// (not configured on this environment, table not yet migrated, or a
// domain with no row) falls straight through to it, same
// never-fail-the-request posture every other Supabase-backed lookup
// in this file already uses (getTransactionByReference(),
// getCustomerById()) — a routing-config lookup must never turn into a
// 500 or block a payment from resolving *some* provider.
//
// Deliberately does NOT implement a second, per-domain override tier
// on top of the platform default (Stripe's own "per-Checkout-Session
// override" of its Default Config) — the existing, unchanged
// `req.body.provider` / `req.query.provider` explicit-override
// precedence at every call site already gives a caller that exact
// capability, per-call, so there's nothing new to build for it here.
async function resolveDomainDefaultProvider(domain) {
  const configured = await getRoutingDefaultProvider(domain);
  return configured || DOMAIN_DEFAULT_PROVIDER[domain];
}

// Task 10 (Korapay-focus partial — see handover.md's "Current focus:
// Korapay only" section): ROUTING_RULES above still only maps an
// abstract `action` string to a provider with zero awareness of
// currency, exactly the gap this task describes. A full fix (pick a
// provider from currency+country, across all providers) isn't
// possible yet — Paystack and Korapay are the only two providers with
// a confirmed currency list (see getSupportedCurrencies() in
// utils/helpers.js); JuicyWay isn't confirmed and we have no working
// keys to verify a guess against.
//
// What this DOES do now: once a provider has been chosen (via explicit
// `provider`, or via `action` -> ROUTING_RULES), if that provider is
// one of the two with a confirmed list, the requested currency is
// checked against it *before* ever calling the provider. A mismatch
// returns a clear 400 naming the currency and the provider — not the
// silent 100-guaranteed-to-fail-downstream behavior the task
// description calls out. For juicyway this check is skipped entirely
// (falls through, same behavior as before this task) since there's
// nothing confirmed yet to validate against — see handover.md's
// Task 10 note for what's left once it has its own confirmed
// currency list. See also Task 51 for the broader domain-based
// (international/African) routing model this task is expected to
// eventually be superseded by.
// Task 11: basic request-shape validation on POST /pay, run before any
// provider is even resolved. Previously the only check here was
// `if (!amount)` — which passed for negative numbers, non-numeric
// strings, etc. — everything else fell through to whichever provider's
// API happened to reject it, usually with a far less specific error.
function assertValidAmount(amount) {
  if (typeof amount !== 'number' || !Number.isFinite(amount) || amount <= 0) {
    const err = new Error(
      `'amount' must be a positive number (received: ${JSON.stringify(amount)})`
    );
    err.statusCode = 400;
    throw err;
  }
}

// Shape-only check (3-letter code) — this is deliberately NOT the same
// thing as assertCurrencySupported below, which checks against a
// specific provider's confirmed-supported list. This one just rejects
// obviously malformed input (e.g. "Naira", "12", "") before routing
// even happens.
function assertValidCurrencyFormat(currency) {
  if (!isValidCurrencyCode(currency)) {
    const err = new Error(
      `'currency' must be a 3-letter code, e.g. 'NGN' (received: ${JSON.stringify(currency)})`
    );
    err.statusCode = 400;
    throw err;
  }
}

// Only enforced for providers confirmed (by reading their
// processPayment() call sites, see utils/helpers.js's
// PROVIDERS_REQUIRING_EMAIL note) to forward the email with no
// fallback default.
function assertValidCustomerEmail(providerName, customer) {
  if (!providerRequiresEmail(providerName)) return;
  if (!isValidEmail(customer?.email)) {
    const err = new Error(
      `'customer.email' is required and must be a valid email address for provider '${providerName}' (received: ${JSON.stringify(customer?.email)})`
    );
    err.statusCode = 400;
    throw err;
  }
}

// Task 12 (idempotency — in-scope half only, see handover.md's note):
// this repo has no persistence layer, so it can't itself remember
// "we already processed reference X" across requests (that needs a
// real decision — DB here, or accept-and-forward only, per the task
// description). What it CAN do without a database: accept the
// client's own reference (already destructured above and forwarded
// as-is if given — see `ref = reference || generateReference(...)`
// below) as the de facto idempotency key, and validate its format
// before forwarding, since a malformed one currently reaches the
// provider and fails there with a less specific error.
//
// Paystack's format restriction is confirmed directly against
// paystack.com/docs/api/errors/transaction/ ("Your transaction
// reference includes an invalid character. Only -,.,= and
// alphanumeric characters are allowed"). Korapay's own primary docs
// (developers.korapay.com/docs/checkout-redirect) only say the
// reference "Must be unique for every transaction" — no character
// restriction stated. Note: a secondary source (a third-party skills
// listing, not developers.korapay.com itself) claimed Korapay
// treats a repeated reference as idempotent and "returns the original
// charge" (i.e. a cached result, not an error) — this was checked
// directly against Korapay's own docs this session and is NOT
// confirmed there; the primary source only states the uniqueness
// requirement, the same as Paystack's, which DOES error on reuse
// ("Duplicate Transaction Reference"). Until Korapay's actual
// reuse behavior is confirmed against a primary source, don't build
// anything (here or elsewhere) that assumes Korapay will silently
// return a cached result for a repeated reference — the safer
// assumption, and the one both providers' primary docs actually
// support, is that a reused reference gets rejected as a duplicate.
// JuicyWay: no format research done this session either (out of the
// narrowed Korapay-focus scope) — non-empty-string is the only check
// applied to it.
function assertValidReferenceFormat(providerName, reference) {
  if (reference === undefined || reference === null) return; // omitted -> generateReference() below produces a safe one

  if (typeof reference !== 'string' || reference.length === 0) {
    const err = new Error(`'reference', if provided, must be a non-empty string (received: ${JSON.stringify(reference)})`);
    err.statusCode = 400;
    throw err;
  }

  if ((providerName || '').toLowerCase() === 'paystack' && !/^[A-Za-z0-9\-.=]+$/.test(reference)) {
    const err = new Error(
      `'reference' contains a character Paystack does not allow — only letters, numbers, '-', '.', and '=' (received: ${JSON.stringify(reference)})`
    );
    err.statusCode = 400;
    throw err;
  }
}

function assertCurrencySupported(providerName, currency) {
  const supported = getSupportedCurrencies(providerName);
  if (!supported) return; // not yet confirmed for this provider — can't validate, don't guess

  const currencyUpper = (currency || '').toUpperCase();
  if (!supported.includes(currencyUpper)) {
    const err = new Error(
      `Currency '${currencyUpper}' is not supported by provider '${providerName}'. Supported: ${supported.join(', ')}`
    );
    err.statusCode = 400;
    throw err;
  }
}

// Task 52/e-2e — the Stripe-precedent capability-status check. Not
// called from any route in this file yet: `collection`/`payout`/
// `banks` (the only capabilities `POST /pay`/`POST /payout`/`GET
// /banks` themselves represent) are already unconditionally `active`
// in practice — these routes exist and work — so there is nothing for
// this function to usefully gate on those paths today. It exists here,
// verified and ready, for whichever future route Task 53/54 adds
// (`/kyc`, a card-issuance endpoint, etc.) to call before doing
// anything else — same "build the piece ahead of the route that needs
// it" posture this repo already used for Task 57/d (resolveCustomer())
// existing a full part before Task 57/e wired it into `/pay`.
//
// A missing/unknown capability status (getCapabilityStatus() returns
// `null` — not configured, table not migrated on this environment, or
// an unrecognized capability name) is treated as NOT active, never as
// a silent pass — same "don't guess, fail closed" posture
// assertCurrencySupported() above takes for an unconfirmed currency
// list, just inverted: here, not knowing means "can't confirm this is
// safe to call," so it blocks rather than lets a request through
// hoping for the best on money-moving infrastructure that regressed
// while the routing-config table was migrating or on
// misconfiguration.
async function assertCapabilityActive(capability) {
  const status = await getCapabilityStatus(capability);
  if (status !== 'active') {
    const err = new Error(
      `Capability '${capability}' is not currently active (status: ${status || 'unknown'}).`
    );
    err.statusCode = 501;
    throw err;
  }
}

// Task 13 (error-handling review half): shared by every catch block
// below. Two kinds of errors reach these catch blocks:
// - Validation errors from the assert* functions above and ApiErrors
//   from handleApiCall (utils/helpers.js) — both already carry a
//   deliberately client-safe message (assert* messages are our own
//   text describing bad input; handleApiCall now only lets a
//   provider-flagged message through, see providerError() and its
//   Task 13 comment in utils/helpers.js).
// - Config/operational errors (missing API key, no base URL configured
//   for a provider) tagged `isConfigError` by getProviderKey() /
//   getProviderBaseUrl() in utils/helpers.js — these describe this
//   server's own setup state, not anything about the request, and
//   telling an external caller exactly which provider's credentials
//   aren't configured is information this API has no reason to give
//   out. Full detail is still in the log line right before each of
//   these catch blocks' calls to this function.
function clientSafeMessage(error, fallback) {
  if (error.isConfigError) {
    return 'Payment service is temporarily unavailable for this provider. Please try again shortly.';
  }
  return error.message || fallback;
}

const getProvider = (name) => {
  switch (name.toLowerCase()) {
    case 'paystack': return new Paystack();
    case 'juicyway': return new Juicyway();
    case 'korapay': return new Korapay();
    default: throw new Error(`Provider '${name}' not supported`);
  }
};

// ==================================================
// 🔔 WEBHOOK HANDLERS
// ==================================================
// Paystack (Task 3), Korapay (Task 4), and Juicyway (Task 5) now do
// real signature/checksum verification — see
// providers/paystack.js#verifyWebhookSignature,
// providers/korapay.js#verifyWebhookSignature, and
// providers/juicyway.js#verifyWebhookSignature.
//
// Raw-body note from Task 2 has now been checked against Paystack's,
// Korapay's, and Juicyway's own official examples and does NOT apply
// to any of the three — all hash/checksum the express.json()-parsed-
// and-re-serialized body (or, for Juicyway, a checksum field inside
// that body), not raw bytes.
// Task 60/d — split out from webhookHandlers below. Stripe's own
// dashboard "resend" doesn't re-derive a new HMAC signature for a
// replay; it re-delivers the *original*, already-verified payload and
// lets the endpoint's own business logic run again. B-Pay has no
// separate "endpoint" to redeliver to (this backend IS the endpoint),
// so the equivalent here is re-running just the post-verification
// side-effect logic against the stored payload — which only works if
// that logic is reachable without a live signature header. Before this
// task, it wasn't: verification and processing were fused into one
// function per provider. This map is exactly that side-effect logic,
// extracted so both the live webhook route (after a real signature
// check) and the new manual-replay route (after loading a
// `signature_valid: true` row) can call the same code, instead of the
// replay route duplicating each switch statement or the live route
// losing its verification step.
const webhookEventProcessors = {
  paystack: async (event, data) => {
    switch (event) {
      case 'charge.success':
        // Per paystack.com/docs/payments/webhooks/, this is the
        // authoritative "payment actually succeeded" signal — more
        // reliable than the client-side redirect/callback. No
        // persistence layer exists yet (see Task 12), so for now this
        // just logs the confirmed transaction; a future task wires
        // this into whatever store Task 12 decides on.
        log(`Paystack charge.success: reference=${data?.reference}, amount=${data?.amount}, status=${data?.status}`);
        break;
      default:
        // Paystack's docs list no dedicated "charge failed" event —
        // failures simply don't raise a webhook, so every other event
        // type here (transfer.*, refund.*, subscription.*, dispute.*,
        // etc.) is just acknowledged and logged for now, not acted on.
        log(`Paystack webhook event '${event}' received, no handler wired yet — logged only`);
    }
  },
  korapay: async (event, data) => {
    switch (event) {
      case 'charge.success':
      case 'charge.failed':
      case 'transfer.success':
      case 'transfer.failed':
      case 'refund.success':
      case 'refund.failed':
        log(`Korapay ${event}: reference=${data?.reference}, amount=${data?.amount}, currency=${data?.currency}, status=${data?.status}`);
        break;
      default:
        log(`Korapay webhook event '${event}' received, no handler wired yet — logged only`);
    }

    // Task 41 — this is now the single Korapay webhook receiver for
    // every multi-tenant app (Korapay's dashboard only ever points at
    // one URL, account-wide). Fan the verified event out to whichever
    // app's `reference` prefix matches, via webhookGateway.js. This
    // call is fire-and-forget-with-recording, not fire-and-wait: it
    // records the event and attempts one immediate forward, but the
    // response to Korapay below happens regardless of that forward's
    // outcome — a failed forward gets retried by index.js's periodic
    // sweep instead of holding Korapay's own webhook delivery hostage
    // (Korapay has its own retry behavior on non-200, which is exactly
    // what this is trying to avoid depending on for correctness — see
    // webhookGateway.js's own file header for the full reasoning,
    // including its one known limitation: in-memory only, not durable
    // across a restart/redeploy yet). Also re-run on a manual replay —
    // that's the whole point of replaying a Korapay event (e.g. a
    // downstream app's own consumer missed the original fanout).
    await handleGatewayEvent(event, data);
  },
  juicyway: async (event, data) => {
    switch (event) {
      case 'payment.session.succeeded':
      case 'payment.session.failed':
        // Per docs.juicyway.com/webhooks, `data.status` is 'success' or
        // 'failed' regardless of which of these two events fired. No
        // persistence layer exists yet (see Task 12).
        log(`Juicyway ${event}: reference=${data?.reference}, amount=${data?.amount}, currency=${data?.currency}, status=${data?.status}`);
        break;
      default:
        log(`Juicyway webhook event '${event}' received, no handler wired yet — logged only`);
    }
  },
};

// Task 60/e — continuous-failure alerting, mirroring Stripe's own
// "auto-disable + notify the account owner after sustained failure"
// pattern (STRIPE_DISCOVERY.md §3; confirmed independently against
// Stripe's live docs and third-party integration guides this session:
// ~3 days of continuous non-2xx/timeout retries, then disable +
// email). B-Pay isn't the one retrying deliveries here — the ten
// providers are, on their own undocumented schedules (Task 60/c
// already flagged this as unconfirmed for Korapay/JuicyWay) — so a
// wall-clock "3 days" isn't a meaningful threshold B-Pay can enforce
// on itself. The B-Pay-appropriate analog is a *consecutive-failure
// streak* on this table's own `status` column: this app has full
// control over that value ('processed' vs 'failed', wired below).
// `WEBHOOK_ALERT_THRESHOLD` overrides the default for a slower-moving
// or noisier provider without a code change.
const WEBHOOK_ALERT_THRESHOLD = parseInt(process.env.WEBHOOK_ALERT_THRESHOLD, 10) || 5;

// Runs a provider's post-verification processor, always resolving the
// `webhook_events` row to a terminal status (this closes the real gap
// Task 60/b's own section flagged: before this leaf, a processor
// throwing left the row stuck at 'received' forever — indistinguishable
// from "still being processed," and invisible to any failure count).
// On failure: marks the row 'failed', checks whether this provider's
// most recent rows are now a run of `WEBHOOK_ALERT_THRESHOLD`
// consecutive failures (a single 'processed' row anywhere in that
// window resets the streak, same "one success clears it" semantics
// Stripe's own retry/disable clock uses), and fires notifyOps() exactly
// once per streak — at the moment the threshold is crossed, not on
// every failure after — so a sustained outage pages once, not on a
// loop. Re-throws the original error either way; the caller's own
// try/catch (POST /webhooks/:provider below) is unchanged and still
// owns the HTTP response.
async function runWebhookProcessor(provider, eventRowId, processor) {
  try {
    await processor();
    await markWebhookEventStatus(eventRowId, 'processed');
  } catch (err) {
    await markWebhookEventStatus(eventRowId, 'failed');

    const recent = await getRecentWebhookOutcomes(provider, WEBHOOK_ALERT_THRESHOLD);
    const isFreshStreak = recent.length === WEBHOOK_ALERT_THRESHOLD && recent.every((status) => status === 'failed');
    if (isFreshStreak) {
      await notifyOps(`${provider} webhook processing: ${WEBHOOK_ALERT_THRESHOLD} consecutive failures`, {
        provider,
        threshold: WEBHOOK_ALERT_THRESHOLD,
        latestError: err.message,
        latestEventRowId: eventRowId,
      });
    }

    throw err;
  }
}

const webhookHandlers = {
  paystack: async (req) => {
    const provider = new Paystack();
    const signature = req.headers['x-paystack-signature'];

    if (!provider.verifyWebhookSignature(req.body, signature)) {
      log(`Paystack webhook signature verification FAILED`, 'error');
      const err = new Error('Invalid webhook signature');
      err.statusCode = 401;
      throw err;
    }

    log(`Paystack webhook signature verified OK`);

    const { event, data } = req.body || {};
    log(`Paystack webhook event: ${event}`);

    // Task 60/b — dedup, per Task 60/c's discovery (no confirmed
    // native event-id for Paystack; fallback `event:reference` key).
    // Recorded only after signature verification succeeds above — see
    // utils/supabase.js's own Task 60/b header note on that flagged
    // gap (a failed-signature attempt gets no row here).
    const dedupeKey = computeProviderEventKey(event, data);
    if (dedupeKey && await isWebhookEventProcessed('paystack', dedupeKey)) {
      log(`Paystack webhook '${dedupeKey}' already processed — short-circuiting, no side effects re-run`);
      return { received: true, duplicate: true };
    }
    const eventRowId = await recordWebhookEvent({
      provider: 'paystack',
      provider_event_id: dedupeKey,
      payload: req.body,
      signature_valid: true,
    });

    await runWebhookProcessor('paystack', eventRowId, () => webhookEventProcessors.paystack(event, data));

    return { received: true };
  },
  korapay: async (req) => {
    const provider = new Korapay();
    const signature = req.headers['x-korapay-signature'];

    if (!provider.verifyWebhookSignature(req.body, signature)) {
      log(`Korapay webhook signature verification FAILED`, 'error');
      const err = new Error('Invalid webhook signature');
      err.statusCode = 401;
      throw err;
    }

    log(`Korapay webhook signature verified OK`);

    const { event, data } = req.body || {};
    log(`Korapay webhook event: ${event}`);

    // Task 60/b — dedup, per Task 60/c's discovery (Korapay's own docs
    // confirm no dedicated event-id field; same `event:reference`
    // fallback webhookGateway.js's own Task 41 computeDedupeKey()
    // already used for its separate multi-tenant-fanout concern — this
    // is B-Pay's own general webhook_events ledger, a distinct table
    // from that gateway's in-memory-only event store, so both dedupe
    // independently on the same key shape rather than sharing state).
    const dedupeKey = computeProviderEventKey(event, data);
    if (dedupeKey && await isWebhookEventProcessed('korapay', dedupeKey)) {
      log(`Korapay webhook '${dedupeKey}' already processed — short-circuiting, no side effects re-run`);
      return { received: true, duplicate: true };
    }
    const eventRowId = await recordWebhookEvent({
      provider: 'korapay',
      provider_event_id: dedupeKey,
      payload: req.body,
      signature_valid: true,
    });

    await runWebhookProcessor('korapay', eventRowId, () => webhookEventProcessors.korapay(event, data));

    return { received: true };
  },
  juicyway: async (req) => {
    const provider = new Juicyway();

    // Unlike Paystack/Korapay, Juicyway has no signature HTTP header --
    // the checksum lives inside the JSON body itself, so the whole
    // parsed body is passed in, not a header value. See
    // providers/juicyway.js#verifyWebhookSignature for the full scheme.
    if (!provider.verifyWebhookSignature(req.body)) {
      log(`Juicyway webhook signature verification FAILED`, 'error');
      const err = new Error('Invalid webhook signature');
      err.statusCode = 401;
      throw err;
    }

    log(`Juicyway webhook signature verified OK`);

    const { event, data } = req.body || {};
    log(`Juicyway webhook event: ${event}`);

    // Task 60/b — dedup. Task 60/c found several candidate id-shaped
    // fields in Juicyway's own sample payloads (`data.id`,
    // `data.transaction_id`, `data.correlation_id`, etc.) but none
    // confirmed as THE dedup key — deliberately using the same safe
    // `event:reference` fallback as Paystack/Korapay above rather than
    // guessing `data.transaction_id` into this codebase unconfirmed;
    // see computeProviderEventKey()'s own header note in
    // utils/helpers.js for the explicit reasoning.
    const dedupeKey = computeProviderEventKey(event, data);
    if (dedupeKey && await isWebhookEventProcessed('juicyway', dedupeKey)) {
      log(`Juicyway webhook '${dedupeKey}' already processed — short-circuiting, no side effects re-run`);
      return { received: true, duplicate: true };
    }
    const eventRowId = await recordWebhookEvent({
      provider: 'juicyway',
      provider_event_id: dedupeKey,
      payload: req.body,
      signature_valid: true,
    });

    await runWebhookProcessor('juicyway', eventRowId, () => webhookEventProcessors.juicyway(event, data));

    return { received: true };
  },
};

// ==================================================
// 🛣️ ROUTES
// ==================================================

// POST /api/pay
//
// Task 42 Part c (per Part b-a-ii's verdict) — protected the same way
// /payout already is. Confirmed via Part b-a-i's own investigation:
// this route has exactly one real caller anywhere across the apps
// that use this backend (Mavins-web's `initialize-payment` Supabase
// Edge Function, server-to-server, secret held in `Deno.env`) — the
// same trusted-caller shape that already justified this middleware on
// /payout. That Edge Function needs updating to actually send
// `X-Internal-Api-Key` — see this task's own handover.md note; this
// commit only covers this backend's own side of the change.
router.post('/pay', requireInternalApiKey, async (req, res) => {
  try {
    // `action` is still accepted in the request body for backward
    // compatibility (old clients may still send it) but is
    // deliberately not read here anymore — see the routing-precedence
    // comment below for why (Task 52/e-2 part a).
    // Task 57/a: `provider_data` is the new namespaced envelope for
    // provider-specific fields that don't belong on the shared
    // canonical core (see handover.md Task 57) -- e.g.
    // `provider_data.juicyway` carries JuicyWay's description/
    // payment_method/order/extended-customer fields. Forwarded as-is;
    // each provider's own processPayment() decides what (if anything)
    // to read from its own namespaced key. Harmless no-op for any
    // provider that doesn't look at it, same as payment_currency/
    // channels below already are for non-Korapay providers.
    //
    // Task 57/e: `provider_data` is deliberately NOT destructured here
    // anymore -- it's read off `resolvedBody` (below, after
    // resolveCustomer() runs) instead of raw `req.body`, since the
    // Customer Vault may fill in some of its nested `customer.*`
    // fields between here and the field-requirements check. `customer`
    // (the canonical `{ email }` core) is unaffected by any of that --
    // Task 57's own envelope rule is that email is always supplied
    // fresh and never vaulted -- so it's still read straight off
    // `req.body` same as before.
    const { action, provider, amount, customer, currency, reference, payment_currency, settlement_currency, channels, default_channel } = req.body;

    log(`Payment Request Received: ${formatPayload(req.body)}`);

    // Task 11: validate request shape before doing anything else —
    // amount must actually be a positive number (not just truthy), and
    // currency (defaulting to NGN like the rest of this handler
    // already does) must at least look like a real 3-letter code.
    // Malformed requests now get a specific 400 instead of falling
    // through to a provider API call that fails confusingly.
    assertValidAmount(amount);
    const resolvedCurrency = currency || 'NGN';
    assertValidCurrencyFormat(resolvedCurrency);

    // Smart Routing: Determine provider from action OR explicit provider field
    //
    // Task 52/e-2, part a (2026-09-08): replaced the old
    // action -> ROUTING_RULES lookup with Task 51's domain-based model
    // for POST /pay specifically (collection capability only — /payout,
    // /payout/verify, /banks are separate leaves, still on the old
    // ROUTING_RULES path, not touched here). Precedence, in order:
    //   1. Explicit `provider` field — client-requested override/
    //      fallback, unchanged from before this change. This already
    //      satisfies e-2's "expose a way to explicitly request a
    //      fallback" requirement for this leaf; no new mechanism
    //      needed for that part.
    //   2. Task 51's per-domain default, via classifyDomain(currency) +
    //      DOMAIN_DEFAULT_PROVIDER above — juicyway for international,
    //      korapay for african_rails.
    // The old `action` field / ROUTING_RULES[action] lookup is
    // deliberately NOT consulted here anymore — Task 51 explicitly
    // states this action-string model is what the domain model
    // supersedes for POST /pay. A client still sending `action` (e.g.
    // 'collect_payment') without an explicit `provider` now gets
    // routed by domain instead of by that old action string; flagging
    // this plainly since it's an intentional behavior change, not an
    // oversight, in case any caller was relying on the old mapping.
    let providerName = provider;
    if (!providerName) {
      const domain = classifyDomain(resolvedCurrency);
      providerName = await resolveDomainDefaultProvider(domain);
    }

    log(`Routing to provider: '${providerName}'`);

    // Task 10 (Korapay-focus partial): reject a currency the resolved
    // provider is confirmed NOT to support, instead of forwarding it
    // and letting the provider API fail with a confusing error (or, in
    // the worst case, silently accepting a currency the provider
    // doesn't actually process correctly). See assertCurrencySupported
    // above for exactly which providers this currently covers.
    assertCurrencySupported(providerName, resolvedCurrency);

    // Task 11: customer.email is required (and must look like an
    // email) for providers whose processPayment() forwards it with no
    // fallback default — see utils/helpers.js's PROVIDERS_REQUIRING_EMAIL
    // note for exactly which providers.
    assertValidCustomerEmail(providerName, customer);

    // Task 12 (in-scope half): if the client supplied their own
    // reference (the de facto idempotency key, since this backend has
    // no persistence layer to enforce one itself), validate its format
    // before forwarding — see assertValidReferenceFormat above for the
    // confirmed-vs-unconfirmed provider rules this currently covers.
    assertValidReferenceFormat(providerName, reference);

    const providerInstance = getProvider(providerName);

    // Task 57/e: run the Customer Vault's resolution order (Task
    // 57/d's resolveCustomer()) before the field-requirements check
    // below, not after -- the whole point of the vault is that a
    // field it fills in should count as "present" for that check, the
    // same way an explicit request field already does. Placed after
    // getProvider() above for the same reason (b)'s own comment
    // already gives: an unrecognized provider name should still fail
    // with the pre-existing "not supported" error first, unaffected
    // by any of this. `req.body.customer_id` (if supplied) is looked
    // up against the `customers` table and merged into a *new*
    // `resolvedBody`, filling only whichever provider_data.<provider>.
    // customer.* fields the caller didn't already supply -- resolution
    // order step 1 (explicit request field always wins) still holds,
    // enforced inside resolveCustomer() itself, not re-checked here.
    // `req.body.save_customer === true` (if present) persists a new
    // vault row from the resolved fields; `savedCustomerId` is the new
    // row's id, or `null` if `save_customer` wasn't set, had nothing
    // vaultable to save, or the save itself failed -- a failed save
    // must not fail or block this payment (same non-blocking posture
    // Task 56/d-3 already established for recordTransaction()), so it
    // is deliberately never awaited into a thrown error here.
    const { resolvedBody, customerId: savedCustomerId } = await resolveCustomer(providerName, req.body);
    const providerData = resolvedBody.provider_data;

    // Task 57/b (3/4): field-requirements registry wired in. Placed
    // after getProvider() above (not before) so a provider name
    // getProvider() itself doesn't recognize still fails with that
    // pre-existing "not supported" error first, unchanged — this
    // check only ever runs for a provider that already resolved.
    // Checked against `resolvedBody` (Task 57/e), not raw `req.body`
    // anymore -- since the registry's own `path` values
    // (utils/fieldRequirements.js) are dot-paths that include
    // `provider_data.<provider>.customer.*` fields, and those are
    // exactly the fields the Customer Vault (Task 57/d, just above)
    // may have already filled in from a vaulted row. A field the
    // vault filled in now correctly counts as present here, instead of
    // still being reported missing just because the caller didn't
    // repeat it explicitly on this particular call -- that's the
    // entire point of resolution-order step 2. Closes the gap Task
    // 57/a's own writeup flagged as still open: previously nothing
    // stopped an incomplete request (e.g. JuicyWay missing its
    // required order/customer fields) from reaching the provider's
    // API and failing there with a less specific error — this returns
    // a clean 400 naming every missing field instead, per Task 57's
    // own "name exactly which field is missing" design. Now enforces
    // something real for Paystack and Korapay too (not just
    // JuicyWay), since (2/4) gave them registry entries first — a
    // provider with no entry at all still gets an empty array back
    // (see getMissingFields's own doc comment) and is completely
    // unaffected, same as before this part.
    const missingFields = getMissingFields(providerName, resolvedBody);
    if (missingFields.length > 0) {
      const err = new Error(
        `Missing required field(s) for provider '${providerName}': ${missingFields.map((f) => f.label).join(', ')}`
      );
      err.statusCode = 400;
      throw err;
    }

    // Task 23: per "Project owner decisions" -> Decision 1 (as
    // corrected), this route's intended caller is now the Supabase
    // Edge Function, which owns reference generation and is expected
    // to always supply its own `reference` (the de facto idempotency
    // key -- see Task 12's own comment above). A missing reference
    // here is no longer the expected/common case it was when the app
    // called this backend directly, so it's now logged as a warning
    // (a possible bug signal -- stale Edge Function code, a malformed
    // call, or a legacy caller) rather than silently accepted. The
    // fallback itself is deliberately NOT removed: Task 23 explicitly
    // scopes this as an audit-and-decide task, not an automatic
    // deletion, and this backend has no persistence layer of its own
    // to confirm nothing still depends on the fallback -- so a
    // request without a reference still succeeds, exactly like
    // before, it's just no longer silent about it.
    if (!reference) {
      log(
        `POST /pay called without a client-supplied reference (provider: '${providerName}') -- falling back to generateReference(). This route's intended caller (the Supabase Edge Function) is expected to always supply its own reference; an internal-API-key-authenticated caller omitting one may indicate stale Edge Function code or a legacy/malformed request, not necessarily a problem.`,
        'warn'
      );
    }
    const ref = reference || generateReference(providerName);

    const paymentData = {
      amount,
      currency: resolvedCurrency,
      reference: ref,
      customer,
      // Optional Dynamic Currency Conversion fields (Korapay-specific
      // today -- see providers/korapay.js). `currency`/`amount` above
      // stays the merchant's own accounting currency (what the caller
      // actually owes); `payment_currency` is what the *payer* sees at
      // checkout, `settlement_currency` is what the merchant is paid
      // out in. Omit both to charge directly in `currency` with no
      // conversion, which is still the default for every other provider.
      payment_currency,
      settlement_currency,
      // Task 30 (Mavins-web) companion: Korapay-specific channel
      // preference -- see developers.korapay.com/docs/checkout-redirect's
      // `channels`/`default_channel` params. Mirrors the
      // payment_currency/settlement_currency pattern immediately above
      // (optional, forwarded as-is, provider decides what to do with
      // them -- see providers/korapay.js for the Korapay-specific
      // handling). Harmless no-op for every other provider today since
      // none of their processPayment() implementations read these keys.
      channels,
      default_channel,
      // Task 57/a: namespaced provider-specific envelope -- see the
      // destructuring comment above for what this carries and why.
      // Task 57/e: sourced from `resolvedBody` (above), not the raw
      // request body, so any Customer Vault fill-ins actually reach
      // the provider's processPayment(), not just the missing-fields
      // check above.
      provider_data: providerData,
    };

    const result = await providerInstance.processPayment(paymentData);

    log(`Payment Success: ${providerName} - ${ref}`);

    // Task 56/d-3-b: best-effort transaction record. Deliberately NOT
    // awaited — recordTransaction() (d-3-a) already never throws, and
    // not awaiting it means a slow or unreachable Supabase insert can
    // never delay this response, per Task 56/d-3's own "must not fail
    // or block the underlying call" decision. Fire-and-forget only.
    //
    // status: 'pending', not 'success' — processPayment() resolving
    // here only confirms the charge was *initialized* with the
    // provider (e.g. Korapay's own /charges/initialize returns a
    // checkout URL for the payer to complete, not a completed
    // payment); real completion is confirmed later, out-of-band, via
    // the provider's webhook (Task 3/4/5) or GET /verify. This status
    // mapping was an explicitly open detail as of d-3-a — resolved
    // here, this session, not assumed beforehand. `amount` (not
    // `resolvedCurrency`'s own pre-conversion value) is the same raw
    // request amount forwarded to the provider above — this table
    // doesn't yet track provider-specific subunit conversion
    // (Task 9/9b's own concern), consistent with `paymentData.amount`
    // itself.
    recordTransaction({
      reference: ref,
      type: 'payment',
      provider: providerName,
      currency: resolvedCurrency,
      amount,
      status: 'pending',
      // Task 45d: JuicyWay's own verify call needs its own UUID
      // (`GET /payments/{id}`), not the merchant reference every other
      // provider's verify accepts — persisted here, off the raw
      // response `processPayment()` already returns un-reshaped, so
      // `GET /verify` can resolve it back out by `reference` later.
      // `undefined` for every other provider — recordTransaction()
      // itself already treats a falsy `provider_reference` as "don't
      // set this column" (see its own comment), so no other call site
      // changes behavior.
      provider_reference: providerName === 'juicyway' ? result?.data?.payment?.id : undefined,
    });

    // Task 61/b: best-effort ledger record, same fire-and-forget
    // posture as recordTransaction() immediately above — see
    // recordBalanceTransaction()'s own header comment (utils/
    // supabase.js) for why `business_id`/`transaction_id` are both
    // left unset here. `type: 'payment'` per the balance_transactions
    // CHECK constraint (migration 0014) — this table's taxonomy is
    // Stripe's own simplified five-value one, not `transactions`'
    // own per-flow `type` value.
    recordBalanceTransaction({
      reference: ref,
      provider: providerName,
      type: 'payment',
      amount,
      currency: resolvedCurrency,
    });

    // Task 57/e: `customer_id` only appears when this call actually
    // saved a new vault row -- per Task 57's own "on save, the
    // response returns the new customer_id so the caller can reuse it
    // next time" text. Omitted (not `null`) on every other call --
    // no `save_customer`, nothing vaultable to save, a save failure,
    // or a call that only *read* an existing customer_id -- rather
    // than adding a field callers would otherwise have to learn to
    // ignore on the common case.
    const responseBody = {
      status: true,
      message: 'Payment initiated successfully',
      provider: providerName,
      reference: ref,
      data: result,
    };
    if (savedCustomerId) {
      responseBody.customer_id = savedCustomerId;
    }

    return res.status(200).json(responseBody);

  } catch (error) {
    log(`Payment Error: ${error.message}`, 'error');
    // Respects error.statusCode when the error carries one (e.g. the
    // 400 from assertCurrencySupported above, Task 10) instead of
    // always answering 500 — same pattern already used by the
    // /webhooks/:provider route below (Task 3). Message goes through
    // clientSafeMessage (Task 13) so a config/operational failure
    // (missing key, no base URL for the resolved provider) doesn't
    // echo its specifics back to the caller.
    return res.status(error.statusCode || 500).json({
      status: false,
      message: clientSafeMessage(error, 'Payment processing failed'),
    });
  }
});

// GET /api/verify?reference=XYZ&provider=paystack
router.get('/verify', async (req, res) => {
  try {
    const { reference, provider } = req.query;

    log(`Verification Request Received: ${formatPayload(req.query)}`);

    if (!reference || !provider) {
      return res.status(400).json({ 
        status: false, 
        message: 'Missing query params: reference, provider' 
      });
    }

    const providerInstance = getProvider(provider);

    // Task 45d: JuicyWay's `verifyTransaction(reference)` can't
    // actually look up by the merchant reference — its own `GET
    // /payments/{id}` (Fetch Payment) takes JuicyWay's own UUID, and
    // List Payments' documented filters don't include lookup-by-
    // merchant-reference (see this leaf's own handover.md entry).
    // Same resolution shape `GET /payout/verify` already uses for its
    // own currency gap (Task 56/d-4): look the external `reference` up
    // in `transactions`, and if this route recorded JuicyWay's own id
    // for it (`POST /pay`, above), pass THAT to verifyTransaction()
    // instead. A miss here (no row, Supabase unreachable, or a
    // reference that predates migration 0009) fails with a clear 404
    // rather than forwarding the merchant reference to JuicyWay's API
    // anyway and letting it 404 there with a confusing provider-side
    // message — this backend can already tell the request is
    // unresolvable without making that call.
    let lookupReference = reference;
    if (provider === 'juicyway') {
      const transaction = await getTransactionByReference(reference);
      if (!transaction || !transaction.provider_reference) {
        const err = new Error(
          `No JuicyWay payment id found for reference '${reference}' — cannot verify.`
        );
        err.statusCode = 404;
        throw err;
      }
      lookupReference = transaction.provider_reference;
    }

    const result = await providerInstance.verifyTransaction(lookupReference);

    log(`Verification Success: ${provider} - ${reference}`);

    return res.status(200).json({
      status: true,
      message: 'Verification successful',
      provider,
      data: result,
    });

  } catch (error) {
    log(`Verification Error: ${error.message}`, 'error');
    // Task 13: found incidentally while reviewing error handling — this
    // route always answered 500 regardless of error.statusCode, unlike
    // POST /pay (fixed by Task 10) and POST /webhooks/:provider (Task 3).
    // A bad provider name here (getProvider() throws a plain Error) or an
    // ApiError from a failed verifyTransaction() call both now get their
    // real status code instead of being flattened to 500. Message goes
    // through the same clientSafeMessage() sanitization as POST /pay.
    return res.status(error.statusCode || 500).json({
      status: false,
      message: clientSafeMessage(error, 'Verification failed'),
    });
  }
});

// ==================================================
// 📶 VTU (AIRTIME/DATA) — telcos.opik.net, Task 58/a, order-of-
// execution step 5, part (b)
// ==================================================
// Five routes, all behind requireInternalApiKey — a wallet-funded
// purchase is not lower-stakes than a payout just because the
// amounts are typically smaller (Task 58/a's own note). Response
// envelope is this repo's own `{ status: 'success'|'error', data }`
// shape (matching /banks, /payout/verify) — NOT telcos.opik.net's raw
// `{ success, data }` shape, same "normalize into our own envelope"
// convention every other provider response already gets below.
//
// Deliberately NOT wired in this part (separate, later
// order-of-execution steps — not an oversight):
//   - step 7: recordTransaction() wiring.
//   - step 8: POST /api/webhooks/telcosopik — blocked separately on
//     the signing-scheme open item (Task 58/i).
//
// businessId — decided this session (2026-09-09): no existing
// mechanism in this codebase resolves a business identity from a
// request. requireInternalApiKey (utils/helpers.js) is a single
// shared secret for one trusted internal caller, not a per-caller/
// per-business credential, and the `businesses` table itself
// (migration 0010) has no dashboard-login column and isn't wired to
// any request-auth path yet (db/SCHEMA.md's own "Not yet built"
// note). So, same as every other caller-supplied identifier this
// repo already uses (`bank_code`/`account_number` on /payout),
// `businessId` is a plain, required, caller-supplied field — body for
// the two POST routes, query for the three GET routes, since GET
// requests conventionally carry no body. Flagged plainly as a
// decision, not a discovered pre-existing convention — a real
// per-caller auth scheme (Task 45/c's still-open dashboard-login
// question) may supersede this later.
function getVtuBusinessId(req) {
  const businessId = req.method === 'GET' ? req.query.businessId : req.body.businessId;
  if (!businessId || typeof businessId !== 'string') {
    const err = new Error(`'businessId' is required and must be a non-empty string (received: ${JSON.stringify(businessId)})`);
    err.statusCode = 400;
    throw err;
  }
  return businessId;
}

// c-1's "first call to any /api/vtu/* route" activation trigger,
// applied uniformly to all five routes below — not just the two
// purchase routes — since every one of them needs an authenticated
// telcos.opik.net client either way, and provisionTelcosOpikAccount()
// is a fast, no-op DB check on every call after the first (its own
// check-then-create). `req.body?.registration` (`{ email, password,
// firstName, lastName, companyName }`) is only actually read by
// provisionTelcosOpikAccount() the first time a given business hits
// any of these routes; every later call ignores it entirely.
// Deliberately sourced from req.body even for the three GET routes
// below — registration includes a password, which has no business
// being logged or cached in a URL/query string. A first-ever call
// that omits `registration` surfaces telcos.opik.net's own
// POST /auth/register validation error back to the caller (via
// providerError(), already client-safe per Task 13) rather than this
// repo re-implementing that validation — the same "no registry entry
// yet" gap flagged in this section's own top comment.
async function getVtuClientForBusiness(businessId, req) {
  await provisionTelcosOpikAccount(businessId, req.body?.registration);
  const apiKey = await resolveTelcosOpikApiKey(businessId);
  return new TelcosOpik(apiKey);
}

// GET /api/vtu/plans?businessId=...&network=MTN&category=data
router.get('/vtu/plans', requireInternalApiKey, async (req, res) => {
  try {
    const businessId = getVtuBusinessId(req);
    const { network, category } = req.query;

    const client = await getVtuClientForBusiness(businessId, req);
    const result = await client.getPlans({ network, category });

    res.json({ status: 'success', data: result });
  } catch (error) {
    log(`VTU plans error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Failed to fetch VTU plans'),
    });
  }
});

// GET /api/vtu/wallet?businessId=...
router.get('/vtu/wallet', requireInternalApiKey, async (req, res) => {
  try {
    const businessId = getVtuBusinessId(req);

    const client = await getVtuClientForBusiness(businessId, req);
    const result = await client.getWallet();

    res.json({ status: 'success', data: result });
  } catch (error) {
    log(`VTU wallet error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Failed to fetch VTU wallet'),
    });
  }
});

// POST /api/vtu/data
// Body: { businessId, planId, phoneNumber, network, registration? }
router.post('/vtu/data', requireInternalApiKey, async (req, res) => {
  try {
    const businessId = getVtuBusinessId(req);
    const { planId, phoneNumber, network } = req.body;

    log(`VTU Data Purchase Request Received: ${formatPayload(req.body)}`);

    // Task 58/g: checked before touching Supabase/telcos.opik.net at
    // all — same "clean 400 naming exactly what's missing, instead of
    // a confusing provider-side failure" motivation as /pay's own
    // getMissingFields() call (Task 57/b), just checked earlier here
    // since there's no provider-resolution step to wait on first.
    const missingFields = getMissingFields('data', req.body);
    if (missingFields.length > 0) {
      const err = new Error(
        `Missing required field(s) for VTU data purchase: ${missingFields.map((f) => f.label).join(', ')}`
      );
      err.statusCode = 400;
      throw err;
    }

    const client = await getVtuClientForBusiness(businessId, req);
    const result = await client.purchaseData({ planId, phoneNumber, network });

    // Task 58 step 7: best-effort transaction record, same fire-and-
    // forget/`'pending'` pattern Task 56/d-3 established for /pay and
    // /payout — not awaited, since recordTransaction() (d-3-a) already
    // never throws, so a slow/unreachable Supabase insert can never
    // delay this route's response.
    //
    // reference: telcos.opik.net generates this itself (docs/guides/
    // 05-purchasing-data-airtime.md's own response shape) — unlike
    // /pay, this route never computes or forwards a caller/repo-side
    // reference up front, so `result.data.reference` is the only
    // value available to record.
    //
    // amount: this route's own request body carries `planId`, not an
    // amount (the plan determines the price) — `result.data.amount` is
    // the only amount known at this point, mirroring the reference
    // situation above.
    //
    // currency: hardcoded 'NGN' — telcos.opik.net's data/airtime rails
    // are Nigeria-only (docs/guides/05-purchasing-data-airtime.md's own
    // title), and neither /vtu/data nor /vtu/airtime accepts a
    // currency field at all (see fieldRequirements.js's 'data'/
    // 'airtime' entries), unlike /pay's multi-currency providers.
    //
    // status: 'pending', not 'success' — the purchase docs explicitly
    // flag that a 200 here isn't confirmed to mean "final" (see that
    // file's own note to confirm via GET /transactions later), same
    // "don't assume completion just because the call resolved" posture
    // /pay's own d-3-b comment gives for Korapay/JuicyWay-style
    // providers.
    //
    // type: 'vtu_data' — this repo has no pre-existing convention for
    // a VTU transaction's `type` value (flagged, not guessed silently,
    // per this box's own note above); distinguished from
    // 'vtu_airtime' below so the two purchase kinds don't collapse
    // into one bare 'payment' value.
    recordTransaction({
      reference: result?.data?.reference,
      type: 'vtu_data',
      provider: 'telcosopik',
      currency: 'NGN',
      amount: result?.data?.amount,
      status: 'pending',
    });

    // Task 61/b: best-effort ledger record, same fire-and-forget
    // posture as recordTransaction() immediately above — see that
    // helper's own header comment (utils/supabase.js) for why
    // `transaction_id` is left unset here. Unlike /pay and /payout,
    // this route DOES have a `businessId` in scope (getVtuBusinessId()
    // above) — passed through as `business_id`, per migration 0014's
    // own note that the VTU routes are the one write path that
    // currently can populate this column. `type: 'payment'`, not
    // `'vtu_data'` — the balance_transactions CHECK constraint
    // (migration 0014) only allows Stripe's own simplified five-value
    // taxonomy (payment/payout/fee/refund/adjustment); a VTU purchase
    // debits a business's balance the same way a payment does, so it
    // maps to `'payment'` here even though `transactions.type` keeps
    // the more specific `'vtu_data'` value for that table's own,
    // per-flow taxonomy.
    recordBalanceTransaction({
      business_id: businessId,
      reference: result?.data?.reference,
      provider: 'telcosopik',
      type: 'payment',
      amount: result?.data?.amount,
      currency: 'NGN',
    });

    res.json({ status: 'success', data: result });
  } catch (error) {
    log(`VTU data purchase error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'VTU data purchase failed'),
    });
  }
});

// POST /api/vtu/airtime
// Body: { businessId, network, phoneNumber, amount, registration? }
router.post('/vtu/airtime', requireInternalApiKey, async (req, res) => {
  try {
    const businessId = getVtuBusinessId(req);
    const { network, phoneNumber, amount } = req.body;

    log(`VTU Airtime Purchase Request Received: ${formatPayload(req.body)}`);

    // Task 58/g — see POST /vtu/data's own comment above for why this
    // check runs before getVtuClientForBusiness(), not after.
    const missingFields = getMissingFields('airtime', req.body);
    if (missingFields.length > 0) {
      const err = new Error(
        `Missing required field(s) for VTU airtime purchase: ${missingFields.map((f) => f.label).join(', ')}`
      );
      err.statusCode = 400;
      throw err;
    }

    const client = await getVtuClientForBusiness(businessId, req);
    const result = await client.purchaseAirtime({ network, phoneNumber, amount });

    // Task 58 step 7: same fire-and-forget/`'pending'` recordTransaction()
    // wiring as POST /vtu/data above — see that route's own comment for
    // the full reasoning on each field. The one difference: this
    // route's request body already carries `amount` directly (no
    // plan-based lookup involved), so the raw request `amount` is used
    // here rather than reading it back off `result.data`, matching how
    // /pay's own d-3-b uses its own request `amount` rather than a
    // provider-echoed value.
    recordTransaction({
      reference: result?.data?.reference,
      type: 'vtu_airtime',
      provider: 'telcosopik',
      currency: 'NGN',
      amount,
      status: 'pending',
    });

    // Task 61/b: same wiring as POST /vtu/data above — see that
    // route's own comment for the full reasoning on `business_id`/
    // `transaction_id`/`type`. This route's request `amount` is used
    // directly (matching its own recordTransaction() call just above),
    // not a provider-echoed value.
    recordBalanceTransaction({
      business_id: businessId,
      reference: result?.data?.reference,
      provider: 'telcosopik',
      type: 'payment',
      amount,
      currency: 'NGN',
    });

    res.json({ status: 'success', data: result });
  } catch (error) {
    log(`VTU airtime purchase error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'VTU airtime purchase failed'),
    });
  }
});

// GET /api/vtu/transactions?businessId=...&limit=20&offset=0
router.get('/vtu/transactions', requireInternalApiKey, async (req, res) => {
  try {
    const businessId = getVtuBusinessId(req);
    const { limit, offset } = req.query;

    const client = await getVtuClientForBusiness(businessId, req);
    const result = await client.getTransactions({
      limit: limit !== undefined ? Number(limit) : undefined,
      offset: offset !== undefined ? Number(offset) : undefined,
    });

    res.json({ status: 'success', data: result });
  } catch (error) {
    log(`VTU transactions error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Failed to fetch VTU transactions'),
    });
  }
});

// ==================================================
// 💰 PER-BUSINESS BALANCE VIEW — GET /api/balance?businessId=...
// (Task 61/c)
// ==================================================
// Thin wrapper around getBusinessBalance() (utils/supabase.js) — see
// that function's own header comment for the sign convention and
// available-vs-pending split logic; this route does no aggregation of
// its own. `requireInternalApiKey` + a caller-supplied `businessId`
// query param, same trust model and same validation shape as the VTU
// routes above (getVtuBusinessId()) — not reused directly (that
// helper's own doc comments and naming are VTU-specific), but
// intentionally the same convention, since no other business-identity
// mechanism exists yet in this codebase (Task 46's dashboard/business
// login is still undesigned).
//
// Response shape: `{ status: 'success', data: [{ currency, available,
// pending }, ...] }` — an empty array is a legitimate "no ledger
// activity yet" response, not an error; `getBusinessBalance()`
// returning `null` (Supabase unreachable/misconfigured, or a query
// failure) surfaces as a 503 here rather than a misleading empty-array
// success, since "unknown" and "genuinely zero" are different things a
// caller building a dashboard on top of this needs to tell apart.
router.get('/balance', requireInternalApiKey, async (req, res) => {
  try {
    const businessId = req.query.businessId;
    if (!businessId || typeof businessId !== 'string') {
      const err = new Error(`'businessId' is required and must be a non-empty string (received: ${JSON.stringify(businessId)})`);
      err.statusCode = 400;
      throw err;
    }

    const balance = await getBusinessBalance(businessId);

    if (balance === null) {
      const err = new Error('Balance lookup unavailable');
      err.statusCode = 503;
      throw err;
    }

    res.json({ status: 'success', data: balance });
  } catch (error) {
    log(`Balance lookup error: ${error.message}`, 'error');
    res.status(error.statusCode || 500).json({
      status: 'error',
      message: clientSafeMessage(error, 'Failed to fetch balance'),
    });
  }
});

// POST /api/webhooks/:provider
// Paystack (Task 3), Korapay (Task 4), Juicyway (Task 5): real
// signature/checksum verification, 401 on mismatch.
router.post('/webhooks/:provider', async (req, res) => {
  const { provider } = req.params;

  try {
    log(`Webhook received for provider '${provider}'`);
    log(`Webhook headers: ${JSON.stringify(req.headers, null, 2)}`);
    log(`Webhook body: ${formatPayload(req.body)}`);

    const handler = webhookHandlers[provider?.toLowerCase()];

    if (!handler) {
      log(`Webhook Error: unknown provider '${provider}'`, 'error');
      return res.status(404).json({ status: false, message: `Unknown webhook provider: ${provider}` });
    }

    await handler(req);

    return res.status(200).json({ status: true, message: 'Webhook received' });

  } catch (error) {
    log(`Webhook Error: ${error.message}`, 'error');
    // Task 13: same clientSafeMessage() sanitization as POST /pay and
    // GET /verify — each webhookHandlers entry instantiates a provider
    // class (e.g. `new Paystack()`), which can throw the same
    // isConfigError-tagged errors on a missing key.
    return res.status(error.statusCode || 500).json({ status: false, message: clientSafeMessage(error, 'Webhook processing failed') });
  }
});

// POST /api/webhooks/:id/replay — Task 60/d
//
// Mirrors Stripe Dashboard's own manual "resend" affordance (migration
// 0016/0017's own comments call this out as the reason `payload` is
// stored in full), scaled to B-Pay's no-dashboard-yet reality (Task 46
// still open) as a plain internal route instead of a UI button.
// `requireInternalApiKey`-gated, same trust model as `/payout`, `/pay`,
// and `GET /api/balance` — this replays real provider side effects
// (Korapay's multi-tenant fanout in particular), so it is exactly as
// sensitive as a live webhook delivery and gets the same protection,
// not a lesser one.
//
// `:id` is a `webhook_events.id` (the row's own uuid primary key), not
// a provider event id — the operator finds it by querying the table
// directly (no list endpoint exists yet; Task 46's dashboard is the
// eventual UI for this).
//
// Deliberately does NOT run the `isWebhookEventProcessed` dedup
// short-circuit that the live route above does — a manual replay's
// entire purpose is to re-run a delivery that may already be marked
// `processed` (e.g. a downstream consumer never actually received the
// original fanout), so the dedup check that protects the live route
// from a provider's own duplicate retries would defeat the feature
// here. This is an explicit operator action behind an internal-only
// credential, not an untrusted inbound delivery — same reasoning
// Stripe's own dashboard resend doesn't dedupe against prior
// deliveries either.
//
// Refuses to replay a row whose `signature_valid` is not `true` — this
// table is designed to eventually also hold failed-verification
// attempts (Task 60/b's own flagged gap, still open), and even once it
// does, an unverified payload must never be fed back through real
// side-effect code just because an operator has an internal API key;
// replay re-runs trusted history, it does not re-verify untrusted
// input.
router.post('/webhooks/:id/replay', requireInternalApiKey, async (req, res) => {
  const { id } = req.params;

  try {
    log(`Manual webhook replay requested for webhook_events.id='${id}'`);

    const row = await getWebhookEventById(id);

    if (!row) {
      log(`Webhook replay: no webhook_events row found for id='${id}'`, 'warn');
      return res.status(404).json({ status: 'error', message: `No webhook event found for id '${id}'` });
    }

    if (row.signature_valid !== true) {
      log(`Webhook replay refused for id='${id}' — signature_valid is not true (${row.signature_valid})`, 'error');
      return res.status(400).json({ status: 'error', message: 'Refusing to replay a webhook event that did not pass signature verification' });
    }

    const processor = webhookEventProcessors[row.provider?.toLowerCase()];
    if (!processor) {
      log(`Webhook replay: no processor registered for provider '${row.provider}' (id='${id}')`, 'error');
      return res.status(400).json({ status: 'error', message: `No replay handler registered for provider '${row.provider}'` });
    }

    const { event, data } = row.payload || {};
    log(`Replaying ${row.provider} webhook '${event}' (webhook_events.id='${id}', originally received ${row.received_at}, previous status '${row.status}')`);

    await processor(event, data);

    await markWebhookEventStatus(row.id, 'processed');

    return res.status(200).json({
      status: 'success',
      message: 'Webhook event replayed',
      data: { id: row.id, provider: row.provider, event, previous_status: row.status },
    });
  } catch (error) {
    log(`Webhook replay error for id='${id}': ${error.message}`, 'error');
    // Best-effort — if the row was found and matched above, mark it
    // failed so the next replay attempt (or Task 60/e's future
    // alerting) can see this one didn't succeed; if it failed before
    // that point (e.g. the lookup itself), there's no row id to mark.
    if (req.params.id) {
      await markWebhookEventStatus(req.params.id, 'failed');
    }
    return res.status(error.statusCode || 500).json({ status: 'error', message: clientSafeMessage(error, 'Webhook replay failed') });
  }
});

export default router;