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

// ==================================================
// Task 52/d-2b (handover.md) — Flutterwave v4 method set, part (2) of
// the 3-way split (v3 methods [done, class above] / v4 OAuth2 methods
// [this class] / runtime-switch design [d-2c, not started]). Per Task
// 55/a's "build both, dynamically switchable" decision, this is a
// SECOND, coexisting class — not a replacement of `Flutterwave` above
// and not a version flag on it. d-2c (the actual runtime switch a
// caller uses to pick one or the other) is a separate, not-yet-built
// leaf; nothing in routes.js selects between these two classes yet.
//
// Endpoints/behavior below come from this repo's own discovery pass
// (handover.md, "Flutterwave — FULL API discovery pass", 2026-09-06),
// fetched directly from developer.flutterwave.com at that time — NOT
// re-fetched fresh this session. Two real, confirmed gaps from that
// pass are carried into this file explicitly rather than guessed
// past — see verifyTransaction()'s and getBanks()'s own comments.
export class FlutterwaveV4 {
  // Module-level (class-static) token cache, shared across every
  // `new FlutterwaveV4()` instance in this process — routes.js's
  // getProvider() constructs a fresh instance per call (confirmed by
  // reading routes.js before writing this), so a per-instance cache
  // would refetch a token on every single request and defeat the
  // whole point of a 10-minute-lived token. This does NOT survive a
  // cold start/process restart — per Task 0's no-DB constraint, that
  // is accepted here exactly as handover.md's own discovery note
  // already flags, not an oversight.
  static _tokenCache = { accessToken: null, expiresAt: 0 };

  constructor() {
    this.clientId = getProviderKey('flutterwave_v4', 'client_id');
    this.clientSecret = getProviderKey('flutterwave_v4', 'client_secret');
    // Only read when a card charge is attempted (see processPayment) —
    // not required to construct this class for non-card flows.
    this.encryptionKey = process.env.FLW_V4_ENCRYPTION_KEY || '';
    this.baseUrl = getProviderBaseUrl('flutterwave_v4');

    // Real, confirmed inconsistency in Flutterwave's own docs (see
    // handover.md's "v4 Authentication" note): the Authentication and
    // Environments pages name `idp.flutterwave.com`, but the same
    // Authentication page's own PHP SDK sample uses a different host
    // (`keycloak.dev-flutterwave.com`) for the identical token
    // request. `idp.flutterwave.com` is used here as the default,
    // since two of three fetched references agree on it — but this is
    // NOT independently verified against a real sandbox credential
    // pair. Override with FLW_V4_TOKEN_URL if a real test shows the
    // other host is actually required.
    this.tokenUrl =
      process.env.FLW_V4_TOKEN_URL ||
      'https://idp.flutterwave.com/realms/flutterwave/protocol/openid-connect/token';

    log('Flutterwave (v4) provider initialized');
  }

  // ==================================================
  // 🔐 OAUTH2 CLIENT-CREDENTIALS TOKEN MANAGER
  // ==================================================
  // Confirmed shape (handover.md "v4 Authentication"): POST the token
  // endpoint with client_id/client_secret/grant_type as a
  // form-urlencoded body (NOT JSON) → {access_token, expires_in,
  // token_type, scope}. Refreshes proactively at a 60-second buffer
  // before expiry, matching Flutterwave's own official Node/Python/
  // PHP samples' documented refresh pattern rather than a naive
  // fetch-once-per-call implementation.
  async _getAccessToken() {
    const now = Date.now();
    const cache = FlutterwaveV4._tokenCache;

    if (cache.accessToken && cache.expiresAt - now > 60_000) {
      return cache.accessToken;
    }

    const body = new URLSearchParams({
      client_id: this.clientId,
      client_secret: this.clientSecret,
      grant_type: 'client_credentials',
    });

    const result = await handleApiCall(async () => {
      const response = await fetch(this.tokenUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: body.toString(),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.access_token) {
        throw providerError(
          responseData.error_description || responseData.error || 'Flutterwave v4 OAuth token request failed'
        );
      }

      return responseData;
    }, 'flutterwave_v4_oauth');

    // Confirmed 600s (10 min) in the worked example fetched during
    // discovery; falling back to that number if a live response ever
    // omits expires_in rather than treating a missing value as "never
    // expires."
    const expiresInMs = (result.expires_in || 600) * 1000;
    FlutterwaveV4._tokenCache = {
      accessToken: result.access_token,
      expiresAt: now + expiresInMs,
    };

    return result.access_token;
  }

  async _authHeaders() {
    const token = await this._getAccessToken();
    return {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    };
  }

  // ==================================================
  // 🔒 CARD FIELD-LEVEL ENCRYPTION (AES-256-GCM) — NOT IMPLEMENTED
  // ==================================================
  // Deliberately left unbuilt this session, per this file's own
  // no-guessing convention on anything that touches real card data or
  // money movement. Discovery confirmed the SCHEME (AES-256-GCM,
  // per-field, with a distinct encryption key plus a 12-character
  // single-use nonce) but this session's own notes do not capture the
  // exact byte-level serialization Flutterwave's official code
  // samples use (e.g. whether the GCM auth tag is concatenated with
  // the ciphertext before base64-encoding, sent as a separate field,
  // or something else) — guessing at that would risk silently sending
  // malformed encrypted card data. A future leaf should re-fetch
  // Flutterwave's own Node.js Web Crypto sample from the Encryption
  // page directly and implement this against the literal sample, not
  // a description of it.
  _encryptCardFieldsNotImplemented() {
    throw providerError(
      'Flutterwave v4 card charges require field-level AES-256-GCM encryption whose exact wire format was not confirmed in this session (Task 52/d-2b) — not implemented. Non-card payment methods (bank_transfer, mobile_money, ussd, wallet) are fully supported by this class.'
    );
  }

  // ==================================================
  // 💳 COLLECTION
  // ==================================================
  async processPayment(data) {
    const ref = data.reference || generateReference('flutterwave-v4');

    // v4's OpenAPI schema confirms `amount` as a decimal in major
    // currency units — see utils/helpers.js#getAmountFormat's
    // 'flutterwave' case, updated this session with that confirmation
    // (same numeric effect as v3's own inferred rule: multiplier 1).
    const amount = convertAmountForProvider(data.amount, 'flutterwave', data.currency);

    if (data.payment_method?.type === 'card') {
      this._encryptCardFieldsNotImplemented();
    }

    // POST /orchestration/direct-charges — the inline-customer/inline-
    // payment_method "Orchestrator" endpoint, confirmed as the actual
    // single-call analog to every other provider's processPayment()
    // in this repo. Deliberately NOT the plain /charges endpoint,
    // which requires pre-created customer_id/payment_method_id records
    // via separate calls first and does not fit this repo's no-DB,
    // single-call model (see handover.md's "v4 Charge creation" note
    // for the full comparison).
    const payload = {
      reference: ref,
      amount,
      currency: data.currency,
      customer: {
        email: data.customer?.email,
        name: data.customer?.name,
        phone: data.customer?.phone || data.customer?.phonenumber,
      },
      // Caller-supplied, method-specific object (e.g.
      // { type: 'bank_transfer', ... } / { type: 'mobile_money', ... }
      // / { type: 'ussd', ... } / { type: 'wallet', ... }) — this repo
      // does not reshape it, since the schema is genuinely
      // method-dependent and reshaping risks silently dropping a
      // required field. Card-type objects are rejected above before
      // reaching this point.
      payment_method: data.payment_method,
      redirect_url: data.redirect_url,
    };

    log(`Flutterwave v4 Payment Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const headers = await this._authHeaders();

      const response = await fetch(`${this.baseUrl}/orchestration/direct-charges`, {
        method: 'POST',
        headers,
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      // Confirmed v4 error envelope: {status: 'failed', error: {...}}
      // on 4xx/5xx, per handover.md's "v4 Errors" note — checked
      // alongside response.ok since a non-2xx without a parseable
      // envelope (e.g. Cloudflare's own non-JSON throttle page, also
      // flagged in that note) must not be treated as success either.
      if (!response.ok || responseData.status === 'failed') {
        throw providerError(responseData.error?.message || responseData.message || 'Flutterwave v4 payment failed');
      }

      // Documented timeout-and-requery pattern (handover.md, same
      // note): an in-flight charge can come back 201 with
      // next_action.type === 'requires_requery' instead of a
      // definitive result. This function does not loop/poll on the
      // caller's behalf — that belongs in routes.js or a future
      // reconciliation job — but logs it prominently so it is never
      // silently mistaken for a final state.
      if (responseData?.data?.next_action?.type === 'requires_requery') {
        log(
          `Flutterwave v4 charge ${ref} returned requires_requery — caller should poll verifyTransaction() after the documented 20s/40s backoff before treating this as final`,
          'warn'
        );
      }

      return responseData;
    }, 'flutterwave_v4');

    log(`Flutterwave v4 Payment Response: ${formatPayload(result)}`);
    return result;
  }

  // Real, confirmed gap — flagged rather than silently guessed past.
  // This session's discovery pass confirmed a retrieve/requery step
  // EXISTS (the Errors page's own timeout-and-requery worked example
  // explicitly says to "poll the retrieve-charge endpoint") but did
  // NOT capture that endpoint's literal path from a fetched reference
  // page. The path below follows this v4 surface's own REST
  // convention exactly (POST .../orchestration/direct-charges creates
  // a resource in that collection; GET .../orchestration/direct-
  // charges/{reference} reads one back by the same reference) but is
  // NOT independently confirmed against a fetched OpenAPI page the way
  // v3's verify_by_reference path above is. Verify against a real
  // sandbox call before trusting this in production.
  async verifyTransaction(reference) {
    log(`Flutterwave v4 Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      const headers = await this._authHeaders();

      const response = await fetch(
        `${this.baseUrl}/orchestration/direct-charges/${encodeURIComponent(reference)}`,
        { method: 'GET', headers }
      );

      const responseData = await response.json();

      if (!response.ok || responseData.status === 'failed') {
        throw providerError(
          responseData.error?.message || responseData.message || 'Flutterwave v4 verification failed'
        );
      }

      return responseData;
    }, 'flutterwave_v4');

    log(`Flutterwave v4 Verification Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 💸 PAYOUT / DISBURSEMENT (Transfer Orchestrator)
  // ==================================================
  // Confirmed the single most currency-fragmented payload shape of any
  // provider in this file (handover.md "v4 Payouts" note): the request
  // body's required fields are entirely currency-dependent via a
  // oneOf+discriminator schema keyed on destination_currency. This
  // method builds the payload per-currency rather than assuming one
  // fixed shape — an unrecognized currency throws rather than sending
  // a guessed body to a live payout endpoint.
  static FULL_KYC_CURRENCIES = ['EUR', 'GBP', 'USD', 'ZAR'];
  static SENDER_KYC_ALSO_REQUIRED = ['EUR', 'GBP', 'USD'];

  async processPayout(data) {
    const ref = data.reference || generateReference('flutterwave-v4-payout');
    const amount = convertAmountForProvider(data.amount, 'flutterwave', data.currency);
    const currency = (data.currency || '').toUpperCase();

    const payload = {
      reference: ref,
      destination_currency: currency,
      amount,
      // instant/deferred/scheduled — a scheduling capability confirmed
      // unique to v4 among every provider already in this file.
      action: data.action || 'instant',
    };

    if (currency === 'NGN') {
      // Minimal shape, confirmed directly from the OpenAPI schema —
      // matches this repo's other NGN-only providers' own payout
      // shape.
      payload.bank = {
        account_number: data.account_number,
        code: data.bank_code,
      };
    } else if (FlutterwaveV4.FULL_KYC_CURRENCIES.includes(currency)) {
      // Full recipient KYC required. Field CATEGORIES are confirmed
      // (first/last name, phone, email, complete postal address) but
      // this session's discovery notes do not give the literal nested
      // key names Flutterwave's own schema uses — `data.recipient` is
      // forwarded through as-is rather than reshaped, so the caller
      // (who has the actual OpenAPI schema or a real sandbox example
      // in hand) controls the exact field names, and this method
      // doesn't silently mis-nest something it isn't certain of.
      if (!data.recipient) {
        throw providerError(
          `Flutterwave v4 payout in ${currency} requires full recipient KYC (name, phone, email, address) — pass it as data.recipient, per Flutterwave's own currency-conditional schema (field-name-level detail not independently confirmed this session; verify against the real OpenAPI schema before shipping).`
        );
      }
      payload.recipient = data.recipient;
      payload.bank = {
        account_number: data.account_number,
        code: data.bank_code,
      };

      if (FlutterwaveV4.SENDER_KYC_ALSO_REQUIRED.includes(currency)) {
        if (!data.sender) {
          throw providerError(
            `Flutterwave v4 payout in ${currency} additionally requires full sender KYC (name, national ID or date of birth, phone, email, address) — pass it as data.sender.`
          );
        }
        payload.sender = data.sender;
      }
    } else {
      // No other currency's recipient schema was confirmed this
      // session — throwing rather than guessing at a shape for, e.g.,
      // GHS/KES/XAF, which the discovery note explicitly lists as
      // "appeared in the schema but not individually detailed."
      throw providerError(
        `Flutterwave v4 payout: no confirmed recipient-schema mapping for currency '${currency}' — only NGN (minimal) and EUR/GBP/USD/ZAR (full KYC) were confirmed against the OpenAPI schema this session. Confirm that currency's own required fields before extending this method.`
      );
    }

    if (data.narration) payload.narration = data.narration;
    // {date_time, timezone} — only meaningful when action is
    // 'scheduled' or 'deferred'; forwarded through unvalidated since
    // the confirmed IANA-timezone enum wasn't captured verbatim this
    // session.
    if (data.disburse_option) payload.disburse_option = data.disburse_option;

    log(`Flutterwave v4 Payout Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const headers = await this._authHeaders();

      const response = await fetch(`${this.baseUrl}/direct-transfers`, {
        method: 'POST',
        headers,
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || responseData.status === 'failed') {
        throw providerError(responseData.error?.message || responseData.message || 'Flutterwave v4 payout failed');
      }

      return responseData;
    }, 'flutterwave_v4');

    log(`Flutterwave v4 Payout Response: ${formatPayload(result)}`);
    return result;
  }

  // Same class of gap as verifyTransaction() above — a real endpoint
  // for reading back a single transfer almost certainly exists on this
  // resource, but its literal path was not captured from a fetched
  // reference page this session. Follows the same REST convention as
  // above (GET on the creating collection, by reference). Does NOT
  // throw on a FAILED transfer status — same deliberate posture as
  // every other provider's own verifyPayout() in this repo, since this
  // method's whole purpose is to learn the real state, failed
  // included; only a genuine API-level rejection throws.
  async verifyPayout(reference) {
    log(`Flutterwave v4 Payout Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      const headers = await this._authHeaders();

      const response = await fetch(`${this.baseUrl}/direct-transfers/${encodeURIComponent(reference)}`, {
        method: 'GET',
        headers,
      });

      const responseData = await response.json();

      if (!response.ok || responseData.status === 'failed') {
        throw providerError(
          responseData.error?.message || responseData.message || 'Flutterwave v4 payout verification failed'
        );
      }

      return responseData;
    }, 'flutterwave_v4');

    log(`Flutterwave v4 Payout Verification Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🏦 BANK LIST — NOT IMPLEMENTED
  // ==================================================
  // Real, confirmed gap, not a guess: this session's discovery pass
  // covered Authentication, Environments, Encryption, Charges, Errors,
  // Webhooks, and Payouts for v4, but never fetched a v4-specific
  // bank/institution-list reference page. Throwing rather than
  // reusing v3's /banks/:country path unverified — v4's Payouts note
  // shows enough surface-shape differences from v3 (fully currency-
  // discriminated payloads, a different host split) that assuming an
  // identical bank-list endpoint would be a guess, not a confirmation.
  // The v3 `Flutterwave` class's own getBanks() above remains the
  // confirmed option for this provider until a future session audits
  // v4's own endpoint directly.
  async getBanks(_currency = 'NGN') {
    throw providerError(
      "Flutterwave v4 bank-list endpoint was not confirmed against a primary source this session (Task 52/d-2b) — use the v3 Flutterwave class's own getBanks() instead until this is audited."
    );
  }

  // ==================================================
  // 🔔 WEBHOOK SIGNATURE VERIFICATION
  // ==================================================
  // Real, confirmed MECHANISM difference from v3's own
  // verifyWebhookSignature() above: v4's canonical "Verifying Webhook
  // Signatures" section states HMAC-SHA256(rawBody, secretHash),
  // base64-encoded — not a plain shared-secret string comparison the
  // way v3's method (and, contradictorily, the SAME v4 page's own
  // later "Examples" section) do it. The HMAC scheme from the
  // canonical section is implemented here, per handover.md's own
  // explicit instruction not to copy the contradictory "Examples"
  // code. This is why this method's signature differs from v3's
  // (rawBody is required here — there is nothing to HMAC without it).
  //
  // A SEPARATE env var (FLW_V4_SECRET_HASH) is used rather than
  // reusing v3's FLW_SECRET_HASH — this session's own discovery notes
  // never confirmed whether the two API generations share one
  // dashboard-configured secretHash value or have independent ones;
  // treating them as independent config until confirmed otherwise is
  // the fail-safe default (worst case, an operator sets the same
  // value in both env vars).
  verifyWebhookSignature(signature, rawBody) {
    const secretHash = process.env.FLW_V4_SECRET_HASH;

    if (!secretHash) {
      log('FlutterwaveV4 verifyWebhookSignature: FLW_V4_SECRET_HASH is not set — rejecting all webhooks', 'error');
      return false;
    }

    if (!signature || typeof signature !== 'string' || !rawBody) return false;

    const computed = crypto.createHmac('sha256', secretHash).update(rawBody).digest('base64');

    const computedBuffer = Buffer.from(computed, 'utf8');
    const signatureBuffer = Buffer.from(signature, 'utf8');

    if (computedBuffer.length !== signatureBuffer.length) return false;

    return crypto.timingSafeEqual(computedBuffer, signatureBuffer);
  }
}

// ==================================================
// Task 52/d-2c (handover.md) — runtime v3/v4 switch, part (3) of the
// 3-way split (v3 methods [done] / v4 OAuth2 methods [done] / this).
// Design decision made HERE, per this leaf's own instruction to
// decide as part of the work rather than leave it open again:
//
// **Combines two of the three options this leaf's own entry offered
// — per-call override AND an env-var default — rather than inventing
// a fourth mechanism.** The third option (the same promote-to-default
// config Task 52/e-2 leaves open for routing fallbacks generally)
// is deliberately NOT built here: that mechanism doesn't exist yet
// anywhere in this codebase (Task 52/e-2 is still unbuilt), so tying
// this switch to it would mean inventing that infrastructure early
// and only for Flutterwave, ahead of the general routing-layer
// rewrite it's actually supposed to belong to. A future e-2 session
// can fold this switch into that mechanism once it exists, without
// changing this function's own external signature (callers already
// pass `version` as plain data).
//
// - **Per-call**: `options.version` ('v3' | 'v4', case-insensitive),
//   e.g. from an incoming request's own field — highest priority,
//   since a specific caller's explicit choice should never be
//   silently overridden by server-side config.
// - **Per-environment default**: `FLUTTERWAVE_VERSION` env var, same
//   values — lets the product owner change the default without a
//   code deploy, satisfying the same "no deploy needed to change
//   behavior" goal Task 52/e-2's own promote-to-default idea is
//   ultimately after, just scoped to this one provider for now.
// - **Hard-coded fallback**: v3, if neither of the above is set.
//   Chosen deliberately, not arbitrarily: v3 is the fully-built,
//   19-case-tested path with a stable single-static-key auth model
//   matching every other provider in this repo; v4 still carries two
//   unresolved doc inconsistencies (the token-endpoint host, the
//   swapped-URL sample bug already worked around) plus two
//   explicitly-inferred, not-fetched-and-confirmed endpoint paths
//   (verifyTransaction/verifyPayout) and two outright-unimplemented
//   methods (card charges, getBanks). Defaulting to the less-certain
//   path would be the wrong failure mode for a payments backend.
//
// An explicitly unrecognized `options.version` (anything other than
// 'v3'/'v4'/unset) THROWS rather than silently falling back — a
// caller that typo'd or sent a stale value deserves a clear error,
// not a silent switch to a different provider version than they
// asked for, which could send a real payment down an unexpected code
// path.
//
// NOT wired into routes.js's getProvider()/ROUTING_RULES — same as
// both classes above, that remains Task 52/e's job. This factory is
// the thing a future e-2 session should call instead of `new
// Flutterwave()` directly, once that rewrite happens.
export function getFlutterwaveProvider(options = {}) {
  const VALID_VERSIONS = ['v3', 'v4'];
  const requested = typeof options.version === 'string' ? options.version.toLowerCase() : undefined;

  if (requested !== undefined && !VALID_VERSIONS.includes(requested)) {
    throw providerError(
      `getFlutterwaveProvider: unrecognized version '${options.version}' — expected 'v3' or 'v4' (omit to use the configured/default version).`
    );
  }

  const envDefault = (process.env.FLUTTERWAVE_VERSION || '').toLowerCase();
  const version = requested || (VALID_VERSIONS.includes(envDefault) ? envDefault : 'v3');

  log(
    `Flutterwave version resolved to '${version}' (${requested ? 'explicit per-call request' : VALID_VERSIONS.includes(envDefault) ? `FLUTTERWAVE_VERSION env default` : 'hard-coded fallback, no per-call or env override set'})`
  );

  return version === 'v4' ? new FlutterwaveV4() : new Flutterwave();
}
