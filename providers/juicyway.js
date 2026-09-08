import fetch from 'node-fetch';
import crypto from 'crypto';
import { log, handleApiCall, getProviderKey, generateReference, formatPayload, getProviderBaseUrl, providerError } from '../utils/helpers.js';

export class Juicyway {
  constructor() {
    this.apiKey = getProviderKey('juicyway', 'secret');
    this.baseUrl = getProviderBaseUrl('juicyway');
    // Juicyway's webhook checksum is NOT keyed with the API key --
    // per docs.juicyway.com/webhooks, the HMAC key is the merchant's
    // "business ID" (a separate credential from the JuicyWay
    // dashboard), not JUICYWAY_API_KEY/JUICYWAY_PUBLIC_KEY.
    // getProviderKey() has no entry for this since it doesn't fit the
    // existing public/secret pattern, so it's read directly here.
    this.businessId = process.env.JUICYWAY_BUSINESS_ID || '';
    log(`Juicyway provider initialized (${process.env.NODE_ENV || 'development'} mode)`);
  }

  // ==================================================
  // 👤 CREATE BENEFICIARY (required before processPayout)
  // ==================================================
  // Task 52/a-1-iv (2026-09-08). Field shapes below are confirmed
  // against a primary source: docs.juicyway.com/transfers/beneficiaries
  // (the Beneficiaries overview page's own "Beneficiary Information"
  // section documents all three required shapes directly). The exact
  // endpoint PATH is the one thing here that is NOT confirmed —
  // `/beneficiaries` is a same-file precedent placeholder, in the same
  // spirit as processPayment()'s own long-standing `/v1/charges`
  // comment below: verify against Juicyway support/dashboard before
  // relying on this outside a sandbox smoke test. See handover.md
  // Task 52/a-1-iv for the full reasoning and Task 45a for the
  // existing open item this feeds into.
  //
  // Deliberately NOT called automatically from processPayout() when a
  // beneficiary_id is missing -- see handover.md Task 52/a-1-iv point
  // 3 for why this is a separate, explicit call instead.
  async createBeneficiary(data) {
    const type = data.type || 'bank_account';
    let details;

    if (type === 'bank_account') {
      if (!data.account_number || !data.account_name || !data.bank_code || !data.currency) {
        throw providerError('Juicyway bank_account beneficiary requires account_number, account_name, bank_code, currency');
      }
      details = {
        account_details: {
          account_number: data.account_number,
          account_name: data.account_name,
          bank_code: data.bank_code,
          currency: data.currency,
        },
      };
    } else if (type === 'crypto_address') {
      if (!data.address || !data.chain || !data.currency) {
        throw providerError('Juicyway crypto_address beneficiary requires address, chain, currency');
      }
      details = {
        crypto_details: {
          address: data.address,
          chain: data.chain,
          currency: data.currency,
        },
      };
    } else if (type === 'interac') {
      if (!data.email || !data.first_name || !data.last_name) {
        throw providerError('Juicyway interac beneficiary requires email, first_name, last_name');
      }
      details = {
        interac_details: {
          email: data.email,
          name: { first_name: data.first_name, last_name: data.last_name },
          ...(data.phone_number && { phone_number: data.phone_number }),
        },
      };
    } else {
      throw providerError(`Unsupported Juicyway beneficiary type: ${type}`);
    }

    const payload = { type, ...details };

    log(`Juicyway Create Beneficiary Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      // ⚠️ Verify exact endpoint path in Juicyway docs -- see this
      // method's own docblock above, same caveat processPayment()
      // carries for /v1/charges.
      const response = await fetch(`${this.baseUrl}/beneficiaries`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok) {
        throw providerError(responseData.message || 'Juicyway beneficiary creation failed');
      }

      return responseData;
    }, 'juicyway');

    log(`Juicyway Create Beneficiary Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🔍 VERIFY PAYOUT
  // ==================================================
  // Task 52/a-2 (2026-09-08). Endpoint located via
  // docs.juicyway.com/llms.txt ("Get payout details", sibling to
  // processPayout()'s confirmed POST /payouts in the same reference
  // tree) -- the dedicated reference page itself would not fetch this
  // session, so `/payouts/{id}` below is inferred from JuicyWay's own
  // consistent sibling pattern (GET /bulk-transfers/{id} is confirmed
  // for bulk payouts), NOT independently confirmed. Same "verify
  // before relying on this outside a sandbox smoke test" caveat as
  // createBeneficiary()'s endpoint above.
  //
  // IMPORTANT, and the reason this does NOT take a `reference` string
  // the way Korapay.verifyPayout does: the worked-example response in
  // processPayout()'s own docblock has no `reference` field at all --
  // only Juicyway's own `id`. So this takes that `id` (whatever
  // processPayout()'s response returned as `data.id`), not the
  // caller's own reference. Callers must persist that id themselves;
  // this codebase does not do so anywhere yet.
  async verifyPayout(id) {
    log(`Juicyway Payout Verification Request for: ${id}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/payouts/${encodeURIComponent(id)}`, {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
      });

      const responseData = await response.json();

      // Same reasoning as Korapay.verifyPayout: this call's whole
      // purpose is to learn the payout's lifecycle state, including a
      // failed one -- so, unlike processPayout(), this does NOT throw
      // on a failed/rejected status. It only throws if Juicyway
      // itself couldn't find/return the payout at all.
      if (!response.ok) {
        throw providerError(responseData.message || 'Juicyway payout verification failed');
      }

      return responseData;
    }, 'juicyway');

    log(`Juicyway Payout Verification Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🏦 GET BANKS (Nigerian bank/institution list)
  // ==================================================
  // Task 52/a-3 (2026-09-08). Fully confirmed against a primary
  // source: docs.juicyway.com/transfers/transfers/list-ngn-banks
  // documents GET /payment-methods/banks directly, with a full worked
  // response example matching the shape returned below.
  //
  // Unlike Korapay.getBanks(currency), this takes NO currency
  // argument -- JuicyWay's docs only expose a Nigerian bank list
  // ("List Nigerian Banks"), and nothing found this session suggests
  // an equivalent endpoint exists for any other country/currency. If
  // a non-NGN bank list is ever needed, that's a product gap to raise
  // with the product owner, not something to paper over here.
  async getBanks() {
    log('Juicyway Banks Request (Nigeria only — see this method\'s own docblock)');

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/payment-methods/banks`, {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
      });

      const responseData = await response.json();

      if (!response.ok) {
        throw providerError(responseData.message || 'Juicyway bank list failed');
      }

      return responseData;
    }, 'juicyway');

    log(`Juicyway Banks Response: ${formatPayload(result)}`);
    return result;
  }

  async processPayment(data) {
    const ref = data.reference || generateReference('juicyway');

    const payload = {
      amount: data.amount,
      email: data.customer?.email,
      reference: ref,
      currency: data.currency,
    };

    log(`Juicyway Payment Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      // Task 45a: was /v1/charges, which doesn't exist on Juicyway's
      // API -- confirmed against docs.juicyway.com, the real
      // payment-initiation endpoint is /payment-sessions.
      const response = await fetch(`${this.baseUrl}/payment-sessions`, {
        method: 'POST',
        headers: {
          // Task 45a: was `Bearer ${this.apiKey}` -- Juicyway's docs
          // (docs.juicyway.com/authentication.md) are explicit that
          // this header is the raw key with no scheme prefix.
          'Authorization': this.apiKey,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok) {
        // Task 13: same reasoning as paystack.js/korapay.js — this message
        // comes straight from Juicyway's own JSON response body, meant for
        // the end user, so mark it safe-to-surface.
        throw providerError(responseData.message || 'Juicyway payment failed');
      }

      return responseData;
    }, 'juicyway');

    log(`Juicyway Payment Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 💸 PROCESS PAYOUT (international payout/disbursement)
  // ==================================================
  // Confirmed against docs.juicyway.com/reference/payouts/initiate-a-payout.md
  // (raw OpenAPI: POST /payouts, 201 on success) and the worked
  // request/response examples on
  // docs.juicyway.com/transfers/transfers/initiate-bank-transfer.md
  // (2026-09-08 session; Task 52/a-1). See handover.md Task 52/a-1 for
  // the full citation trail -- summarizing the parts that shape this
  // code:
  // - Unlike Korapay's processPayout, this endpoint does NOT take raw
  //   bank_code/account_number -- it takes a `beneficiary` object
  //   referencing a beneficiary resource created ahead of time via
  //   Juicyway's separate Beneficiaries API. Creating/resolving that
  //   beneficiary from raw account details is Task 52/a-1-iv, not yet
  //   implemented -- so this method requires the caller to already
  //   have a beneficiary id, and fails loudly rather than guessing at
  //   an inline-creation shape that hasn't been confirmed yet.
  // - Both worked examples on that page include a `pin` field
  //   (transfer PIN) despite the page's own <ParamField> markup not
  //   clearly marking it required -- treated as required here since
  //   its presence in every example is the stronger signal.
  // - Amount is in minor units (the docs say so explicitly: "Transfer
  //   amount in minor units (e.g., cents, kobo)") -- same subunit
  //   rule Task 49/a already cited for collection, independently
  //   confirmed here for payout rather than assumed. NOT run through
  //   convertAmountForProvider() -- callers pass minor units directly,
  //   matching this file's own processPayment() convention.
  // - The response's `status` is documented as "pending" in both
  //   worked examples; no documented synchronous "failed" outcome the
  //   way Korapay's payout response has one (see
  //   korapay.js#processPayout's own comment on that asymmetry) -- so,
  //   unlike Korapay's implementation, there is no post-hoc
  //   `data.status === 'failed'` check here. Final outcome presumably
  //   arrives via webhook; callers must not treat this method's return
  //   value as "payout completed", same caveat Korapay's docblock
  //   states for the same reason.
  async processPayout(data) {
    const ref = data.reference || generateReference('juicyway-payout');

    const beneficiaryId = data.beneficiary_id || data.beneficiary?.id;
    if (!beneficiaryId) {
      // Task 52/a-1-iv: nothing in this codebase creates a Juicyway
      // beneficiary from raw account details yet. Failing loudly here
      // rather than silently trying to synthesize one against an
      // unconfirmed shape.
      throw providerError(
        'Juicyway payouts require a pre-created beneficiary_id (see handover.md Task 52/a-1-iv — raw bank_code/account_number is not accepted by this endpoint)'
      );
    }

    const pin = data.pin || process.env.JUICYWAY_PAYOUT_PIN;
    if (!pin) {
      throw providerError('Juicyway payouts require a transfer pin (pass data.pin or set JUICYWAY_PAYOUT_PIN)');
    }

    const payload = {
      amount: data.amount,
      beneficiary: {
        id: beneficiaryId,
        type: data.beneficiary?.type || 'bank_account',
      },
      description: data.narration || data.description || 'Payout from Mavins',
      destination_currency: data.destination_currency || data.currency,
      pin,
      reference: ref,
      source_currency: data.source_currency || data.currency,
      ...(data.fee_charged_to && { fee_charged_to: data.fee_charged_to }),
    };

    log(`Juicyway Payout Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/payouts`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok) {
        throw providerError(responseData.message || responseData.data?.reason || 'Juicyway payout failed');
      }

      log(`Juicyway Payout accepted — status: '${responseData.data?.status}' (this is Juicyway's acknowledgement the request was received, NOT final confirmation the transfer completed — see this method's own comment)`);

      return responseData;
    }, 'juicyway');

    log(`Juicyway Payout Response: ${formatPayload(result)}`);
    return result;
  }

  async verifyTransaction(reference) {
    log(`Juicyway Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      // Task 45a: endpoint path here is intentionally UNCHANGED for
      // now. Docs confirm the real verify endpoint is
      // GET /payments/{id} -- but that takes Juicyway's own internal
      // id, not the merchant `reference` this method actually
      // receives, and this repo has no id-from-reference lookup yet
      // (that's Task 45d, still open). Swapping the path here without
      // that lookup would just trade one wrong endpoint for a
      // differently-wrong one, so /v1/charges/${reference} stays as
      // a known-still-broken placeholder until 45d lands. The
      // Authorization header fix below is independent of this and
      // safe to land now.
      const response = await fetch(`${this.baseUrl}/v1/charges/${reference}`, {
        method: 'GET',
        headers: {
          // Task 45a: was `Bearer ${this.apiKey}` -- see the same fix
          // + citation in processPayment() above.
          'Authorization': this.apiKey,
          'Content-Type': 'application/json',
        },
      });

      const responseData = await response.json();

      if (!response.ok) {
        throw providerError(responseData.message || 'Juicyway verification failed');
      }

      return responseData;
    }, 'juicyway');

    log(`Juicyway Verification Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🔔 WEBHOOK SIGNATURE VERIFICATION
  // ==================================================
  // Confirmed directly against docs.juicyway.com/webhooks.md
  // (2026-08-27 session; nothing about this scheme was known before
  // this task). Materially different from both Paystack's and
  // Korapay's schemes:
  // - There is NO signature HTTP header at all. The checksum travels
  //   INSIDE the JSON body as a `checksum` field alongside `event`
  //   and `data` -- so this method takes the whole parsed body, not a
  //   header value.
  // - The HMAC key is the merchant's "business ID", not the secret
  //   API key used for REST calls (see constructor comment above).
  // - The signed string is `${event}|${json_encoded_data}`, where
  //   `data` must be JSON-encoded with keys in alphabetical order --
  //   the docs explicitly warn "the encoded data must exclude the
  //   checksum field and be in alphabetical order" and show a nested
  //   example (customer/merchant/etc. sub-objects) that is itself
  //   alphabetized at every level. Plain JSON.stringify() preserves
  //   insertion order, not alphabetical order -- a naive
  //   JSON.stringify(data) (which is literally what Juicyway's own
  //   Node.js doc example does, despite importing
  //   `json-stable-stringify` and never calling it -- an apparent bug
  //   in their own sample) would silently produce the wrong hash for
  //   any payload whose keys weren't already alphabetized by the
  //   sender. stableStringify() below sorts keys recursively to match
  //   the documented (not the buggy sample) behavior.
  // - The digest is hex, uppercase: the docs' own sample checksum
  //   ("32762AE880695AE7343A649CB9C36CA6FF83AA258A139804AEF7D73B421DE097")
  //   is uppercase hex, and the Python/Node examples both explicitly
  //   uppercase their digest. The PHP example lowercases both sides
  //   before comparing instead -- an inconsistency across Juicyway's
  //   own language examples -- but uppercase is the more consistent
  //   signal (two of three examples, plus the sample value itself),
  //   so that's what's implemented here. Uppercasing the incoming
  //   checksum too before comparing makes this tolerant of either
  //   case regardless.
  // - Only one documented event pair exists so far:
  //   payment.session.succeeded / payment.session.failed. The docs
  //   also note: "In sandbox, successful transactions remain pending.
  //   Only failure events are sent" -- worth remembering for Task 14's
  //   manual test pass, since a sandbox test can't exercise the
  //   success path via a real webhook this way.
  verifyWebhookSignature(payload) {
    if (!payload || typeof payload !== 'object') return false;
    const { checksum, event, data } = payload;
    if (!checksum || !event) return false;

    const message = `${event}|${stableStringify(data)}`;
    const expected = crypto
      .createHmac('sha256', this.businessId)
      .update(message)
      .digest('hex')
      .toUpperCase();

    const expectedBuffer = Buffer.from(expected, 'utf8');
    const checksumBuffer = Buffer.from(String(checksum).toUpperCase(), 'utf8');
    if (expectedBuffer.length !== checksumBuffer.length) return false;
    return crypto.timingSafeEqual(expectedBuffer, checksumBuffer);
  }
}

// Recursively serializes a value with object keys in alphabetical
// order at every nesting level. This is specific to matching
// Juicyway's documented webhook checksum encoding (see
// verifyWebhookSignature above) -- not a general-purpose utility, so
// it's kept local to this file rather than added to utils/helpers.js.
function stableStringify(value) {
  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(',')}]`;
  }
  const keys = Object.keys(value).sort();
  const entries = keys.map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`);
  return `{${entries.join(',')}}`;
}