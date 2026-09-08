import express from 'express';
import { Paystack } from './providers/paystack.js';
import { Juicyway } from './providers/juicyway.js';
import { Korapay } from './providers/korapay.js';
import { log, formatPayload, generateReference, getSupportedCurrencies, isValidCurrencyCode, isValidEmail, providerRequiresEmail, requireInternalApiKey, classifyDomain } from './utils/helpers.js';
import { recordTransaction, getTransactionByReference } from './utils/supabase.js';
import { handleGatewayEvent } from './webhookGateway.js';

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
      providerName = DOMAIN_DEFAULT_PROVIDER[domain];
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
        providerName = DOMAIN_DEFAULT_PROVIDER[domain];
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
      providerName = DOMAIN_DEFAULT_PROVIDER[domain];
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
    // across a restart/redeploy yet).
    await handleGatewayEvent(event, data);

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
    const { action, provider, amount, customer, currency, reference, payment_currency, settlement_currency, channels, default_channel, provider_data } = req.body;

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
      providerName = DOMAIN_DEFAULT_PROVIDER[domain];
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
      provider_data,
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
    });

    return res.status(200).json({
      status: true,
      message: 'Payment initiated successfully',
      provider: providerName,
      reference: ref,
      data: result,
    });

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
    const result = await providerInstance.verifyTransaction(reference);

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

export default router;