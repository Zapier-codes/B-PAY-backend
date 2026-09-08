import fetch from 'node-fetch';
import crypto from 'crypto';
import { log, handleApiCall, getProviderKey, generateReference, formatPayload, getProviderBaseUrl, convertAmountForProvider, providerError } from '../utils/helpers.js';

// Task 52/d-2 (handover.md) — Flutterwave v3 method set, part (1) of
// the 3-way split this leaf's own entry lays out (v3 methods / v4
// OAuth2 methods / runtime-switch design). Only part (1) is built
// this session, per this file's own mandatory task-splitting rule
// (build one part per session, leave the rest explicitly
// not-started). v4 and the runtime switch are NOT in this file —
// a future session adding them should add v4's methods alongside
// these, not replace them (Task 55/a's "build both, dynamically
// switchable" decision), most likely behind a second class or a
// version flag on this one, a decision explicitly left open for
// whoever picks up part (2)/(3).
//
// Endpoints below confirmed against developer.flutterwave.com's own
// v3.0.0 reference pages and the official flutterwave-node-v3 SDK's
// own documentation (github.com/Flutterwave/Node-v3), fetched fresh
// this session (2026-09-08) — not carried over from training-data
// memory. Per-method confidence notes are inline where a detail is
// inferred from examples rather than stated outright, matching this
// file's own existing convention (see korapay.js's own asymmetry
// notes) rather than presenting every claim as equally certain.
//
// NOT wired into routes.js's getProvider() or ROUTING_RULES — that's
// Task 52/e's job (the routing-layer rewrite), deliberately separate
// from building the provider itself.
export class Flutterwave {
  constructor() {
    this.secretKey = getProviderKey('flutterwave', 'secret');
    this.baseUrl = getProviderBaseUrl('flutterwave');
    log('Flutterwave (v3) provider initialized');
  }

  // ==================================================
  // 💳 COLLECTION
  // ==================================================
  async processPayment(data) {
    const ref = data.reference || generateReference('flutterwave');

    // Amount-unit rule: see utils/helpers.js#getAmountFormat's own
    // 'flutterwave' case for the confidence note (inferred from
    // worked examples, not a literal doc statement — flag before
    // fully trusting in production).
    const amount = convertAmountForProvider(data.amount, 'flutterwave', data.currency);

    // POST /v3/payments (Flutterwave "Standard" checkout flow) —
    // confirmed against developer.flutterwave.com/docs/
    // flutterwave-standard-1 and /reference/endpoints/charge's own
    // worked examples. `customer` nested exactly as shown there;
    // `redirect_url` required per the same page.
    const payload = {
      tx_ref: ref,
      amount,
      currency: data.currency,
      redirect_url: data.redirect_url,
      customer: {
        email: data.customer?.email,
        name: data.customer?.name,
        phonenumber: data.customer?.phone || data.customer?.phonenumber,
      },
    };

    // Optional checkout-page branding — passed through only if the
    // caller supplies it, same "don't guess a default, forward what's
    // given" principle Korapay's own optional-field handling above
    // uses (korapay.js#processPayment's channels/DCC comments).
    if (data.customizations) {
      payload.customizations = data.customizations;
    }

    log(`Flutterwave Payment Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/payments`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      // Flutterwave v3's envelope uses a STRING status ("success" /
      // "error"), confirmed directly from every worked example
      // fetched this session — unlike Korapay/Paystack's boolean
      // `status: true/false`. Checking `!== 'success'` here
      // (not a falsy/truthy check) matters: a non-empty error string
      // is still truthy, so a `!responseData.status` check copied from
      // the other two providers would silently treat failures as
      // success.
      if (!response.ok || responseData.status !== 'success') {
        throw providerError(responseData.message || 'Flutterwave payment failed');
      }

      return responseData;
    }, 'flutterwave');

    log(`Flutterwave Payment Response: ${formatPayload(result)}`);
    return result;
  }

  async verifyTransaction(reference) {
    log(`Flutterwave Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      // GET /v3/transactions/verify_by_reference?tx_ref=... — confirmed
      // via developer.flutterwave.com/reference/verify-transaction-with-tx_ref.
      // Deliberately NOT /v3/transactions/:id/verify, which takes
      // Flutterwave's own internal numeric `id`, not the merchant
      // `reference` this method's signature (and every route in this
      // codebase that calls verifyTransaction(reference)) actually
      // passes — using the id-based path would silently break that
      // shared interface for this provider only.
      const response = await fetch(
        `${this.baseUrl}/transactions/verify_by_reference?tx_ref=${encodeURIComponent(reference)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      if (!response.ok || responseData.status !== 'success') {
        throw providerError(responseData.message || 'Flutterwave verification failed');
      }

      return responseData;
    }, 'flutterwave');

    log(`Flutterwave Verification Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 💸 PAYOUT / DISBURSEMENT
  // ==================================================
  async processPayout(data) {
    const ref = data.reference || generateReference('flutterwave-payout');
    const amount = convertAmountForProvider(data.amount, 'flutterwave', data.currency);

    // POST /v3/transfers — confirmed against developer.flutterwave.com/
    // reference/endpoints/transfers and /docs/making-payments/transfers/
    // overview. Flat top-level shape — a REAL, confirmed difference
    // from Korapay's own processPayout(), whose destination fields are
    // nested under a `destination` object (see korapay.js's own Task 42
    // Part B-a comment on that). Not an inconsistency to "fix" — the
    // two providers' APIs are just genuinely shaped differently.
    const payload = {
      account_bank: data.bank_code,
      account_number: data.account_number,
      amount,
      currency: data.currency,
      narration: data.narration,
      reference: ref,
    };

    if (data.debit_currency) {
      payload.debit_currency = data.debit_currency;
    }
    if (data.callback_url) {
      payload.callback_url = data.callback_url;
    }

    log(`Flutterwave Payout Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/transfers`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || responseData.status !== 'success') {
        throw providerError(responseData.message || 'Flutterwave payout failed');
      }

      // Same posture as korapay.js#processPayout(): outer
      // `status: "success"` only confirms Flutterwave ACCEPTED the
      // request, not that money moved — the real lifecycle lives in
      // `data.status` (NEW/SUCCESSFUL/FAILED per Flutterwave's own
      // worked examples). This function's job ends at "accepted"; it
      // does not confirm completion. No webhook handler or automatic
      // verification-call wiring exists for this provider yet —
      // verifyPayout() below exists but nothing calls it
      // automatically (Task 52/e, not built this session).
      if (typeof responseData.data?.status === 'string' && responseData.data.status.toUpperCase() === 'FAILED') {
        throw providerError(responseData.data?.complete_message || responseData.message || 'Flutterwave payout failed');
      }

      log(`Flutterwave Payout accepted — transaction status: '${responseData.data?.status}' (acknowledgement only, not final confirmation — see this function's own comment)`);

      return responseData;
    }, 'flutterwave');

    log(`Flutterwave Payout Response: ${formatPayload(result)}`);
    return result;
  }

  // Real, confirmed asymmetry vs. Korapay's own verifyPayout(reference)
  // — flagged explicitly rather than silently worked around or
  // guessed past. Every primary source checked this session
  // (developer.flutterwave.com's own Transfers reference, the
  // official Node SDK's Transfer.get_a_transfer docs) confirms
  // GET /v3/transfers/:id takes Flutterwave's own internal numeric
  // transfer id. There is no confirmed reference-based single-transfer
  // lookup endpoint the way /transactions/verify_by_reference exists
  // for collections above — this session did not find one and is not
  // guessing one into existence.
  //
  // Practical effect: until this is resolved (or Task 52/e's routing
  // layer is designed to carry the numeric id forward instead of a
  // merchant reference), callers of this method must pass the numeric
  // `id` from processPayout()'s own response (`data.id`) as the
  // `reference` argument — passing an arbitrary merchant-chosen
  // reference string here will 404. This is a real interface gap
  // between this provider and Korapay's, not a bug in this file.
  async verifyPayout(reference) {
    log(`Flutterwave Payout Verification Request for id: ${reference}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/transfers/${encodeURIComponent(reference)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      // Same deliberate divergence from processPayout() as Korapay's
      // own verifyPayout(): this function's whole purpose is to learn
      // the transfer's real state, including a failed one, so it does
      // NOT throw on data.status === 'FAILED' — only a genuine
      // API-level rejection (bad id, auth failure, non-2xx) throws.
      if (!response.ok || responseData.status !== 'success') {
        throw providerError(responseData.message || 'Flutterwave payout verification failed');
      }

      return responseData;
    }, 'flutterwave');

    log(`Flutterwave Payout Verification Response — transaction status: '${result.data?.status}'`);
    return result;
  }

  // ==================================================
  // 🏦 BANK LIST
  // ==================================================
  // Real, confirmed interface difference from Korapay's own
  // getBanks(currency): Flutterwave's GET /v3/banks/:country
  // (developer.flutterwave.com, Node-v3's own banks.md, both fetched
  // this session) is keyed by a 2-letter COUNTRY code, not a currency
  // code, and only 6 countries are documented as supported. This
  // method's own signature stays currency-shaped to match every other
  // provider's getBanks(currency) call site in routes.js, translating
  // internally — a currency outside this confirmed map throws rather
  // than guessing a country.
  static CURRENCY_TO_BANK_COUNTRY = {
    NGN: 'NG',
    GHS: 'GH',
    KES: 'KE',
    UGX: 'UG',
    ZAR: 'ZA',
    TZS: 'TZ',
  };

  async getBanks(currency = 'NGN') {
    const country = Flutterwave.CURRENCY_TO_BANK_COUNTRY[(currency || '').toUpperCase()];

    if (!country) {
      throw providerError(
        `Flutterwave bank list: no confirmed country mapping for currency '${currency}' — confirmed currencies are ${Object.keys(Flutterwave.CURRENCY_TO_BANK_COUNTRY).join(', ')}`
      );
    }

    log(`Flutterwave Banks Request for currency: ${currency} (country: ${country})`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/banks/${encodeURIComponent(country)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      if (!response.ok || responseData.status !== 'success') {
        throw providerError(responseData.message || 'Flutterwave bank list failed');
      }

      return responseData;
    }, 'flutterwave');

    log(`Flutterwave Banks Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🔔 WEBHOOK SIGNATURE VERIFICATION
  // ==================================================
  // Real, confirmed mechanism difference from both Korapay's and
  // Paystack's own verifyWebhookSignature() — Flutterwave's
  // `verif-hash` header (developer.flutterwave.com/docs/webhooks and
  // Flutterwave's own blog post on the topic, both fetched this
  // session) is a plain shared-secret STRING set once on the
  // dashboard and echoed back verbatim on every webhook call — NOT a
  // per-payload HMAC digest the way Korapay's `x-korapay-signature`
  // or Paystack's own header are. Flutterwave's own official examples
  // (Node, PHP) both do a direct string comparison, not a hash
  // computation — at least one third-party blog post found during
  // this session's research computes an HMAC instead, which would
  // reject every genuine Flutterwave webhook; not followed here. This
  // method therefore takes only the header value, not the body —
  // there is nothing to hash.
  //
  // Reads FLW_SECRET_HASH directly (new env var — not yet in
  // render.yaml, a manual product-owner step, same class of action as
  // every other secret noted in handover.md) rather than through
  // getProviderKey(), since this is a separate webhook-only secret
  // the merchant sets arbitrarily on Flutterwave's dashboard, not
  // part of the public/secret API-key pair getProviderKey() models.
  verifyWebhookSignature(signature) {
    const configuredHash = process.env.FLW_SECRET_HASH;

    if (!configuredHash) {
      // Fail closed — same posture as requireInternalApiKey() and
      // every other secret-comparison in this codebase. An unset
      // secret must never be silently treated as "verification not
      // required."
      log('Flutterwave verifyWebhookSignature: FLW_SECRET_HASH is not set — rejecting all webhooks', 'error');
      return false;
    }

    if (!signature || typeof signature !== 'string') return false;

    const configuredBuffer = Buffer.from(configuredHash, 'utf8');
    const signatureBuffer = Buffer.from(signature, 'utf8');

    if (configuredBuffer.length !== signatureBuffer.length) return false;

    return crypto.timingSafeEqual(configuredBuffer, signatureBuffer);
  }
}
