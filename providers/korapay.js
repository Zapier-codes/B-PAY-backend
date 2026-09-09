import fetch from 'node-fetch';
import crypto from 'crypto';
import { log, handleApiCall, getProviderKey, generateReference, formatPayload, getProviderBaseUrl, convertAmountForProvider, providerError } from '../utils/helpers.js';

export class Korapay {
  constructor() {
    this.secretKey = getProviderKey('korapay', 'secret');
    this.baseUrl = getProviderBaseUrl('korapay');
    log('Korapay provider initialized');
  }

  async processPayment(data) {
    const ref = data.reference || generateReference('korapay');
    // Confirmed (Task 7) Korapay wants base currency units, not
    // subunits — convertAmountForProvider is a confirmed no-op here,
    // but routing through it (Task 9, partial) makes that rule
    // explicit and enforced in code rather than only documented in a
    // comment, and keeps this call site consistent with Paystack's.
    const amount = convertAmountForProvider(data.amount, 'korapay', data.currency);

    // Korapay's initialize-charge endpoint requires the payer's details
    // nested under a `customer` object -- a flat top-level `email` field
    // is rejected. See https://developers.korapay.com/docs/checkout-redirect
    const payload = {
      amount,
      currency: data.currency,
      reference: ref,
      customer: {
        email: data.customer?.email,
        name: data.customer?.name,
      },
    };

    // Dynamic Currency Conversion (DCC): only attached when the caller
    // supplies both fields (Korapay requires both together, and both
    // must be currencies Korapay itself supports). `currency` above
    // stays what we're charging in; `payment_currency` is shown to the
    // payer at checkout, `settlement_currency` is what we get paid out
    // in. See https://developers.korapay.com/docs/dynamic-currency-conversion
    if (data.payment_currency && data.settlement_currency) {
      payload.payment_currency = data.payment_currency;
      payload.settlement_currency = data.settlement_currency;
    }

    // Payment method preference (Task 30, Mavins-web companion): only
    // forwarded when the caller supplies `channels` -- an array of
    // Korapay channel strings (bank_transfer, card, pay_with_bank,
    // mobile_money -- confirmed against
    // developers.korapay.com/docs/checkout-redirect's own parameter
    // table). Omitted entirely otherwise, which leaves Korapay's own
    // default channel-selection behavior untouched (same "don't guess,
    // let the provider decide" principle as the currency-validation
    // work in Task 10). `default_channel` only makes sense alongside
    // `channels` per Korapay's own docs ("must also be specified in
    // the channels parameter"), so it's dropped if `channels` wasn't
    // also provided, rather than sent alone and possibly rejected.
    if (Array.isArray(data.channels) && data.channels.length > 0) {
      payload.channels = data.channels;
      if (data.default_channel) {
        payload.default_channel = data.default_channel;
      }
    }

    log(`Korapay Payment Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      // Real endpoint per Korapay's docs is
      // {baseUrl}/api/v1/charges/initialize -- this was previously
      // /transactions/charge, which doesn't exist on Korapay's API.
      const response = await fetch(`${this.baseUrl}/api/v1/charges/initialize`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay payment failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Payment Response: ${formatPayload(result)}`);
    return result;
  }

  async verifyTransaction(reference) {
    log(`Korapay Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      // Real endpoint per Korapay's docs is
      // {baseUrl}/api/v1/charges/:reference (GET, path param) -- this
      // was previously /transactions/verify?reference=, which doesn't
      // exist on Korapay's API.
      const response = await fetch(
        `${this.baseUrl}/api/v1/charges/${encodeURIComponent(reference)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay verification failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Verification Response: ${formatPayload(result)}`);
    return result;
  }


  // ==================================================
  // 💸 PAYOUT / DISBURSEMENT
  // ==================================================
  // Korapay's disburse endpoint for sending money OUT to bank
  // accounts or mobile money wallets. This is the outbound flow
  // (paying listeners, refunds, vendor settlements) vs the inbound
  // collection flow in processPayment() above.
  //
  // Endpoint: POST /api/v1/transactions/disburse
  // Docs: developers.korapay.com/docs/payout-via-api
  //
  // Required fields:
  //   - amount (number): in base currency units (same as collection)
  //   - currency (string): 3-letter code, e.g. 'NGN'
  //   - reference (string): unique transaction reference
  //   - bank_code (string): recipient bank code (from Korapay's bank list)
  //   - account_number (string): recipient account number
  //   - narration (string): description/purpose of the payout
  //
  // Optional fields:
  //   - customer (object): { name, email } — for notification/receipt
  //   - payment_method (string): 'bank_transfer' | 'mobile_money' | etc.
  //
  // The "tag" from Nova Bank maps directly to account_number here.
  // Nova Bank is a virtual-account provider; their "tag" IS the
  // account number Korapay's disburse endpoint expects. No separate
  // Nova Bank API call is needed — Korapay handles the full rail.
  //
  // Task 42 Part B-a (handover.md) — CRITICAL payload-shape fix. Every
  // real payout call this code made before this fix almost certainly
  // failed outright: Korapay's real Payout API requires the entire
  // destination-specific payload nested under a single `destination`
  // object, with `destination.type` present (defaults to
  // 'bank_account' per Korapay's own client library docs if omitted,
  // but sent explicitly here anyway — no reason to rely on an
  // undocumented-in-the-official-reference default when the value is
  // always known at call time) and `destination.customer.email`
  // required, not optional. Independently verified against
  // developers.korapay.com/docs/payout-via-api AND a community Elixir
  // client library's own published type spec (two independent
  // sources agreeing, not one) before writing this — both confirm the
  // exact same shape: `destination: { type, amount, currency,
  // narration, bank_account: { bank, account }, customer: { email,
  // name?, phone? } }`. The previous flat top-level payload
  // (`amount`, `currency`, `bank_code`, `account_number` all as
  // siblings of `reference`) was never a real Korapay payout request
  // shape at any point — this was a genuine bug, not a schema change
  // on Korapay's side.
  async processPayout(data) {
    const ref = data.reference || generateReference('korapay-payout');
    const amount = convertAmountForProvider(data.amount, 'korapay', data.currency);

    // customer.email is REQUIRED by Korapay's real schema (unlike the
    // old flat payload, which treated the whole customer object as
    // optional) — fail loudly here rather than letting Korapay reject
    // the request with a less specific error further downstream.
    if (!data.customer?.email) {
      throw providerError('customer.email is required for Korapay payouts');
    }

    const payload = {
      reference: ref,
      destination: {
        type: data.payment_method === 'mobile_money' ? 'mobile_money' : 'bank_account',
        amount,
        currency: data.currency,
        narration: data.narration || 'Payout from Mavins',
        bank_account: {
          bank: data.bank_code,
          account: data.account_number,
        },
        customer: {
          email: data.customer.email,
          ...(data.customer.name && { name: data.customer.name }),
          ...(data.customer.phone && { phone: data.customer.phone }),
        },
      },
    };

    log(`Korapay Payout Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/api/v1/transactions/disburse`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      // Confirmed directly against developers.korapay.com/docs/payout-via-api
      // (its own "Payout Response" example) — NOT the earlier flagged
      // guess that `status` itself was a string. The real shape is
      // TWO levels: `responseData.status` (top-level) IS a genuine
      // boolean — "did Kora accept this API call" — and the check
      // below was already correct for that. What was missing entirely:
      // `responseData.data.status`, a SEPARATE string field — the
      // *transaction's* own lifecycle state (`"processing"`,
      // `"success"`, or presumably `"failed"` — only `"processing"`
      // is shown in Kora's own example, since a payout is rarely
      // resolved synchronously). This code never looked at that field
      // at all before now.
      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay payout failed');
      }

      // `"processing"` is the NORMAL, EXPECTED outcome here, not a
      // problem — Kora's own docs are explicit that a payout is
      // confirmed asynchronously ("Receive confirmation via webhook
      // when the payout is completed" / "Query the transaction to get
      // the status") and warn AGAINST treating an ambiguous outcome as
      // failed without verifying first ("Handling Unexpected Request
      // Errors": an unexpected error "may have been accepted and
      // processed by Kora" regardless — verify, don't assume). This
      // function's job ends at "Kora accepted the request" — it does
      // NOT confirm the money actually moved. Callers of processPayout()
      // must not treat this return value as "payout completed"; the
      // real outcome arrives via webhook or a later Payout Verification
      // API call, neither of which exists in this codebase yet (see
      // handover.md's own note on this gap).
      //
      // The one synchronous outcome this DOES treat as a real,
      // immediate failure: `data.status === 'failed'`. Not shown in
      // Kora's own documented example (which only shows `"processing"`),
      // but a same-request synchronous rejection (e.g. an immediately
      // invalid destination) is a plausible outcome for a `status:
      // true` / `data.status: 'failed'` combination — outer `status`
      // only confirms the API call itself was well-formed and
      // accepted, not that the transfer will succeed. Treating this as
      // silent success would be a real-money bug, not a cosmetic one.
      if (responseData.data?.status === 'failed') {
        throw providerError(responseData.data?.message || responseData.message || 'Korapay payout failed');
      }

      log(`Korapay Payout accepted — transaction status: '${responseData.data?.status}' (this is Kora's acknowledgement that the request was received, NOT final confirmation the transfer completed — see this function's own comment)`);

      return responseData;
    }, 'korapay');

    log(`Korapay Payout Response: ${formatPayload(result)}`);
    return result;
  }

  // Task 42 "the missing verification call" — split into i/ii per
  // direct instruction. Part i = this method only, built here. Part
  // ii = wiring it into an actual route, NOT built this session.
  // Directly answers processPayout()'s own flagged gap: "no Payout
  // Verification API call... no way to ever learn a 'processing'
  // payout's true final outcome."
  //
  // Endpoint confidence, stated explicitly rather than left implicit:
  // this path is NOT a directly-quoted string from Korapay's own
  // docs the way processPayout()'s request/response shapes are (that
  // page never states the single-payout verify path outright, only
  // links to a separate anchor-based API reference this session
  // couldn't resolve to a literal URL). It's a strong pattern-match
  // instead, evidenced by a real, directly-confirmed sibling: Kora's
  // own Bulk Payouts docs show `POST .../transactions/disburse/bulk`
  // creates a batch and `GET .../transactions/bulk/:batch_reference`
  // verifies it — the same "transactions" resource family
  // processPayout() already POSTs to. Applying that same create/
  // verify pairing to the single (non-bulk) case, dropping the
  // "bulk/" segment: `GET .../transactions/{reference}`. Recommend
  // one real sandbox call to confirm this before trusting it in
  // production — flagged here so that verification step isn't
  // silently skipped later.
  async verifyPayout(reference) {
    log(`Korapay Payout Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/api/v1/transactions/${encodeURIComponent(reference)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      // Same two-level shape as processPayout()'s own response,
      // confirmed this session — outer `status` boolean = "did Kora
      // accept/find this request", inner `data.status` string = the
      // transaction's real lifecycle state. Unlike processPayout(),
      // a verify call's whole PURPOSE is to learn that lifecycle
      // state, including 'failed' — so this function does NOT throw
      // on `data.status === 'failed'` the way processPayout() does;
      // a failed payout is a normal, expected, successfully-verified
      // answer to "what happened to this payout", not an error
      // calling this function. Only a genuine API-level rejection
      // (bad reference, auth failure, etc. — outer `status: false`
      // or a non-2xx) throws here.
      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay payout verification failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Payout Verification Response — transaction status: '${result.data?.status}'`);
    return result;
  }

  // ==================================================
  // 🏦 BANK LIST (helper for payout recipient setup)
  // ==================================================
  // Returns the list of supported banks with their Korapay codes.
  // Callers use this to map a user's selected bank name → bank_code
  // for processPayout(). Cached in-memory for 1 hour by default.
  async getBanks(currency = 'NGN') {
    log(`Korapay Banks Request for currency: ${currency}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/api/v1/banks?currency=${encodeURIComponent(currency)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay bank list failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Banks Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 💳 CARD ISSUING — cardholder, card, retrieval/list, fund, withdraw
  // ==================================================
  // Task 53/b — the five unambiguous methods flagged as buildable in
  // Task 53/a's discovery pass (handover.md). Deliberately NOT
  // included here: suspend/status (`PATCH .../cards/:ref/status`) and
  // the events log (`GET .../cards/:ref/events`) — Task 53/a found two
  // real, unresolved self-contradictions in Korapay's own docs for
  // those two endpoints (`action: suspend` vs `deactivate`; `/api/i/`
  // vs `/api/v1/` path segment) and this file's standing rule is to
  // confirm/test an ambiguous payload or path rather than guess one.
  // Those two stay TODO-flagged, separate methods, until a sandbox
  // call or direct Korapay confirmation resolves them.
  //
  // Scope, confirmed Task 53/a: virtual cards only, USD only,
  // MasterCard/Visa (defaults to MasterCard if `brand` omitted).
  // Requires Live Mode + Korapay's own "Issuing" access grant — not
  // something this code can provision itself.

  // POST /api/v1/cardholders — KYC-shaped payload. This is a separate
  // KYC surface from Task 53/d's PaymentPoint verification (this
  // platform's own verification-of-record) — creating a Korapay
  // cardholder does not substitute for, or get substituted by, that
  // record. `country_identity` uses `bvn` for Nigerian cardholders,
  // `national_id` otherwise, per Korapay's own docs.
  async createCardholder(data) {
    const payload = {
      first_name: data.first_name,
      last_name: data.last_name,
      email: data.email,
      phone: data.phone,
      address: data.address,
      identity: data.identity,
      selfie: data.selfie,
      country_identity: data.country_identity,
    };

    log(`Korapay Create Cardholder Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/api/v1/cardholders`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay cardholder creation failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Create Cardholder Response: ${formatPayload(result)}`);
    return result;
  }

  // POST /api/v1/cards — `currency` is USD-only per Korapay's own
  // docs; `type` field exists but only `virtual` is actually accepted
  // right now (docs: "only virtual cards can be issued for now") even
  // though `physical` is listed as an option — hard-coded here rather
  // than left to the caller, since sending `physical` would be a
  // guaranteed-reject per Korapay's own stated current limitation.
  // Fires `issuing.card_creation.success` on completion; this call's
  // own response returns `status: 'pending'` — same "accepted, not
  // yet confirmed" posture as processPayout() above. Callers must not
  // treat this return value as "card ready" — final truth is the
  // webhook or a follow-up getCard().
  async createCard(data) {
    const ref = data.reference || generateReference('korapay-card');

    const payload = {
      currency: 'USD',
      amount: data.amount,
      card_holder_reference: data.card_holder_reference,
      reference: ref,
      type: 'virtual',
      brand: data.brand || 'mastercard',
    };

    log(`Korapay Create Card Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/api/v1/cards`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay card creation failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Create Card Response — status: '${result.data?.status}' (pending/accepted, not final — see this method's own comment): ${formatPayload(result)}`);
    return result;
  }

  // GET /api/v1/cards/:reference — single card. Response includes
  // PAN/CVV per Korapay's own docs — sensitive; this method returns
  // Korapay's response as-is, so any caller that logs it must not log
  // it verbatim the way this file's other formatPayload() log lines
  // do elsewhere. Deliberately NOT logging the raw response body here
  // for that reason (unlike every other method in this file).
  async getCard(reference) {
    log(`Korapay Get Card Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/api/v1/cards/${encodeURIComponent(reference)}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay card retrieval failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Get Card Response received for: ${reference} (body not logged — contains PAN/CVV, see this method's own comment)`);
    return result;
  }

  // GET /api/v1/cards — list, filterable by status/type/start_date/
  // end_date per Korapay's own docs. Same PAN/CVV sensitivity as
  // getCard() above applies to each list entry — not logged verbatim.
  async listCards(filters = {}) {
    const params = new URLSearchParams();
    if (filters.status) params.set('status', filters.status);
    if (filters.type) params.set('type', filters.type);
    if (filters.start_date) params.set('start_date', filters.start_date);
    if (filters.end_date) params.set('end_date', filters.end_date);
    const qs = params.toString();

    log(`Korapay List Cards Request: ${qs || '(no filters)'}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/api/v1/cards${qs ? `?${qs}` : ''}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
        }
      );

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay card list failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay List Cards Response received (${result.data?.length ?? 'unknown'} cards; body not logged — see getCard()'s own PAN/CVV note)`);
    return result;
  }

  // POST /api/v1/cards/:reference/fund — `reference` here is the
  // FUNDING transaction's own idempotency reference, distinct from
  // the card's own `:reference` path param — confirmed Task 53/a,
  // flagging inline since the naming collision is a real footgun.
  // Fires `issuing.card_funding.success`/`.failed`. Response includes
  // a `fee` field; the fee schedule itself isn't documented anywhere
  // Task 53/a could find — not computed or validated client-side here.
  async fundCard(cardReference, data) {
    const fundingRef = data.reference || generateReference('korapay-card-fund');

    const payload = {
      reference: fundingRef,
      amount: data.amount,
      description: data.description,
    };

    log(`Korapay Fund Card Request for ${cardReference}: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/api/v1/cards/${encodeURIComponent(cardReference)}/fund`,
        {
          method: 'POST',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify(payload),
        }
      );

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay card funding failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Fund Card Response for ${cardReference}: ${formatPayload(result)}`);
    return result;
  }

  // POST /api/v1/cards/:reference/withdraw — same request shape as
  // fundCard() (`reference`, `amount`, `description`); `reference`
  // here is likewise the withdrawal transaction's own idempotency
  // reference, not the card's. Fires
  // `issuing.card_withdrawal.success`/`.failed`. CRITICAL, confirmed
  // Task 53/a from Korapay's own sample response: the returned
  // `amount` is NET OF FEE, not the requested amount (sample:
  // `amount: 10` requested, `fee: 1`, `amount: 9` returned) — this
  // method does not attempt to reconcile that itself; any caller that
  // needs the originally-requested amount must keep it from its own
  // input, not read it back off this response.
  async withdrawCard(cardReference, data) {
    const withdrawalRef = data.reference || generateReference('korapay-card-withdraw');

    const payload = {
      reference: withdrawalRef,
      amount: data.amount,
      description: data.description,
    };

    log(`Korapay Withdraw Card Request for ${cardReference}: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/api/v1/cards/${encodeURIComponent(cardReference)}/withdraw`,
        {
          method: 'POST',
          headers: {
            'Authorization': `Bearer ${this.secretKey}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify(payload),
        }
      );

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Korapay card withdrawal failed');
      }

      return responseData;
    }, 'korapay');

    log(`Korapay Withdraw Card Response for ${cardReference} — net amount returned: ${result.data?.amount} (requested: ${data.amount}, see this method's own comment on fee deduction): ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🔔 WEBHOOK SIGNATURE VERIFICATION
  // ==================================================
  // Confirmed directly against developers.korapay.com/docs/webhooks
  // (2026-08-27 session). Important difference from Paystack: the
  // `x-korapay-signature` header is a hex-encoded HMAC-SHA256 of
  // ONLY the `data` object from the payload — NOT the full body like
  // Paystack. Korapay's own official example hashes
  // `JSON.stringify(req.body.data)`, so this method takes the full
  // parsed body and hashes just its `.data` field, matching that
  // exactly. Like Paystack, this is over the express.json()-parsed-
  // and-re-serialized body, not raw bytes — Korapay's own official
  // examples (Node and PHP alike) do the same re-serialization, so
  // Task 2's raw-body concern doesn't apply here either.
  verifyWebhookSignature(body, signature) {
    if (!signature) return false;
    const hash = crypto
      .createHmac('sha256', this.secretKey)
      .update(JSON.stringify(body?.data))
      .digest('hex');
    const hashBuffer = Buffer.from(hash, 'utf8');
    const sigBuffer = Buffer.from(signature, 'utf8');
    if (hashBuffer.length !== sigBuffer.length) return false;
    return crypto.timingSafeEqual(hashBuffer, sigBuffer);
  }
}