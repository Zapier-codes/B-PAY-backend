import fetch from 'node-fetch';
import crypto from 'crypto';
import { log, handleApiCall, getProviderKey, generateReference, formatPayload, getProviderBaseUrl, convertAmountForProvider, providerError } from '../utils/helpers.js';

export class Paystack {
  constructor() {
    this.secretKey = getProviderKey('paystack', 'secret');
    this.baseUrl = getProviderBaseUrl('paystack');
    log('Paystack provider initialized');
  }

  async processPayment(data) {
    const ref = data.reference || generateReference('paystack');
    // Was: toSubUnit(data.amount, data.currency) — now routed through
    // the per-provider helper (Task 9, partial) so this call site
    // doesn't have to know Paystack-specific unit rules itself.
    const amountInKobo = convertAmountForProvider(data.amount, 'paystack', data.currency);

    const payload = {
      email: data.customer?.email,
      amount: amountInKobo,
      currency: data.currency,
      reference: ref,
    };

    log(`Paystack Payment Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/transaction/initialize`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        // Task 13: Paystack's own responseData.message is provider-authored
        // and meant to be shown to the end user — mark it safe-to-surface so
        // handleApiCall's sanitizer (utils/helpers.js) passes it through
        // instead of replacing it with a generic message.
        throw providerError(responseData.message || 'Paystack initialization failed');
      }

      return responseData;
    }, 'paystack');

    log(`Paystack Payment Response: ${formatPayload(result)}`);
    return result;
  }

  async verifyTransaction(reference) {
    log(`Paystack Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/transaction/verify/${encodeURIComponent(reference)}`,
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
        throw providerError(responseData.message || 'Paystack verification failed');
      }

      return responseData;
    }, 'paystack');

    log(`Paystack Verification Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🏦 TRANSFER RECIPIENT (helper for processPayout)
  // ==================================================
  // Confirmed directly against paystack.com/docs/api/transfer-recipient/
  // and paystack.com/docs/transfers/creating-transfer-recipients/ (Task
  // 52/c, 2026-09-08 session) — unlike Korapay's processPayout, which
  // takes a raw bank_code/account_number pair inline, Paystack requires
  // a recipient to exist first (POST /transferrecipient), returning a
  // recipient_code that the actual transfer references. Paystack's own
  // docs note a duplicate account_number returns the existing record
  // rather than erroring, so calling this on every payout is safe and
  // does not create duplicate recipients.
  async createTransferRecipient(data) {
    const payload = {
      type: data.recipient_type || 'nuban',
      name: data.customer?.name || data.account_name || data.account_number,
      account_number: data.account_number,
      bank_code: data.bank_code,
      currency: data.currency,
    };

    log(`Paystack Create Transfer Recipient Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/transferrecipient`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Paystack transfer recipient creation failed');
      }

      return responseData;
    }, 'paystack');

    log(`Paystack Create Transfer Recipient Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 💸 PAYOUT (transfer)
  // ==================================================
  // Confirmed directly against paystack.com/docs/api/transfer/ and
  // paystack.com/docs/transfers/single-transfers/ (Task 52/c,
  // 2026-09-08 session). `amount` is in minor units (kobo for NGN,
  // pesewas for GHS — Paystack's own docs state this explicitly for
  // transfers, same subunit rule Task 9 already applies to
  // processPayment), so this goes through convertAmountForProvider()
  // like every other method in this file.
  //
  // Important caveat, flagged rather than hidden: Paystack's own docs
  // state the returned transfer `status` will be `"pending"` ONLY if
  // the Transfers OTP requirement is disabled on the integration's
  // dashboard — otherwise status comes back `"otp"` and the transfer
  // is stuck until a human finalizes it with a one-time code sent to
  // the account owner (a separate `/transfer/finalize_transfer`
  // endpoint, NOT implemented here since a server-side integration has
  // no way to receive or supply that OTP). This method does not treat
  // `status === 'otp'` as an error — it's a real, documented Paystack
  // response, not a bug — but callers should not assume `'otp'` means
  // the same thing as Korapay's `'processing'`; unlike Korapay's
  // payout, this transfer will NOT complete on its own until OTP is
  // disabled account-side or someone finalizes it manually.
  async processPayout(data) {
    const ref = data.reference || generateReference('paystack-payout');
    const amount = convertAmountForProvider(data.amount, 'paystack', data.currency);

    const recipientResult = await this.createTransferRecipient(data);
    const recipientCode = recipientResult.data?.recipient_code;

    if (!recipientCode) {
      throw providerError('Paystack did not return a recipient_code — cannot initiate transfer');
    }

    const payload = {
      source: 'balance',
      amount,
      recipient: recipientCode,
      reference: ref,
      reason: data.narration || data.reason || 'Payout from Mavins',
    };

    log(`Paystack Payout Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/transfer`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.secretKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      // Per Paystack's own docs, the HTTP status/outer `status` boolean
      // indicates whether the API call itself succeeded — it does NOT
      // indicate the transfer completed. The transfer's real lifecycle
      // state lives in `data.status` (`pending`, `otp`, `success`, or
      // `failed` — Paystack's "Managing Transfers" page documents all
      // four), same two-level pattern as Korapay's processPayout.
      if (!response.ok || !responseData.status) {
        throw providerError(responseData.message || 'Paystack payout failed');
      }

      if (responseData.data?.status === 'failed') {
        throw providerError(responseData.data?.message || responseData.message || 'Paystack payout failed');
      }

      log(`Paystack Payout accepted — transfer status: '${responseData.data?.status}' (see this function's own comment on the 'otp' status specifically)`);

      return responseData;
    }, 'paystack');

    log(`Paystack Payout Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🔎 PAYOUT VERIFICATION
  // ==================================================
  // Confirmed directly against paystack.com/docs/transfers/bulk-transfers/
  // ("Verify via polling" section, which documents this same endpoint
  // for the single-transfer case too) — GET /transfer/verify/{reference}.
  // Like Korapay's verifyPayout, a `failed` outcome here is a normal,
  // successfully-verified answer, not an error calling this function —
  // only a genuine API-level rejection throws.
  async verifyPayout(reference) {
    log(`Paystack Payout Verification Request for: ${reference}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/transfer/verify/${encodeURIComponent(reference)}`,
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
        throw providerError(responseData.message || 'Paystack payout verification failed');
      }

      return responseData;
    }, 'paystack');

    log(`Paystack Payout Verification Response — transfer status: '${result.data?.status}'`);
    return result;
  }

  // ==================================================
  // 🏦 BANK LIST (helper for payout recipient setup)
  // ==================================================
  // Confirmed directly against paystack.com/docs/transfers/creating-transfer-recipients/
  // — GET /bank?currency=XXX. The same endpoint doubles as the mobile-
  // money telco list for GHS/KES when `type=mobile_money` is also
  // passed (that page's own "Mobile money" section); not wired here
  // since this platform's Paystack payout path is bank-account-only
  // for now — a future leaf, not guessed at.
  async getBanks(currency = 'NGN') {
    log(`Paystack Banks Request for currency: ${currency}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(
        `${this.baseUrl}/bank?currency=${encodeURIComponent(currency)}`,
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
        throw providerError(responseData.message || 'Paystack bank list failed');
      }

      return responseData;
    }, 'paystack');

    log(`Paystack Banks Response: ${formatPayload(result)}`);
    return result;
  }

  // ==================================================
  // 🔔 WEBHOOK SIGNATURE VERIFICATION
  // ==================================================
  // Confirmed directly against paystack.com/docs/payments/webhooks/
  // (2026-08-27 session): the `x-paystack-signature` header is a
  // hex-encoded HMAC-SHA512 of the event payload, keyed with the
  // Paystack secret key. Paystack's own official Node example hashes
  // `JSON.stringify(req.body)` — the body **after** Express's
  // `express.json()` has parsed and re-serialized it — not the raw
  // request bytes. That's a real fragility (re-serialization can
  // diverge from the original bytes for edge cases like key
  // ordering or unicode escaping — several third-party guides flag
  // exactly this), but it's what Paystack's own docs demonstrate, so
  // this method follows the primary source exactly rather than
  // switching to `req.rawBody`. Task 2's raw-body concern turned out
  // to be unnecessary for Paystack specifically once confirmed
  // directly — no `express.json({ verify })` change was needed here.
  // If signature mismatches ever show up in practice, that
  // re-serialization edge case is the first thing to check.
  verifyWebhookSignature(body, signature) {
    if (!signature) return false;
    const hash = crypto
      .createHmac('sha512', this.secretKey)
      .update(JSON.stringify(body))
      .digest('hex');
    // Constant-time compare where possible (falls back to false on
    // length mismatch, which crypto.timingSafeEqual requires anyway).
    const hashBuffer = Buffer.from(hash, 'utf8');
    const sigBuffer = Buffer.from(signature, 'utf8');
    if (hashBuffer.length !== sigBuffer.length) return false;
    return crypto.timingSafeEqual(hashBuffer, sigBuffer);
  }
}