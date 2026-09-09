import fetch from 'node-fetch';
import { log, handleApiCall, getProviderBaseUrl, formatPayload, providerError } from '../utils/helpers.js';
import { getApiKeyRow, insertApiKeyRow, vaultCreateSecret } from '../utils/supabase.js';

// ==================================================
// 📶 TELCOS OPIK — VTU (airtime/data) client (Task 58/b)
// ==================================================
// Mirrors the exact class-per-provider shape Korapay/Paystack already
// use in this repo (constructor + one async method per operation) —
// no new pattern introduced. Every method built on handleApiCall() +
// providerError() from utils/helpers.js, same as every existing
// provider file — no new error-handling mechanism.
//
// Response envelope for every telcos.opik.net endpoint is
// `{ success: boolean, data }` (confirmed, docs/guides/03 through
// 06) — NOT Korapay/Paystack's `{ status: bool, ... }` shape. Checks
// below test `responseData.success`, not `.status`, for that reason.
//
// Credentials — deliberately NOT the single-shared-key
// getProviderKey('telcosopik', 'secret') pattern every other provider
// constructor uses. Task 58/c's 2026-09-09 revision makes this a
// per-business credential (B-Pay provisions a real, individual
// telcos.opik.net account per business, via provisionTelcosOpikAccount()
// below) — there is no single provider-wide secret to read from an
// env var. The constructor takes the already-resolved API key
// directly; wiring "look up business X's key" into a call site is
// step 5's job (routes.js, via the revised
// getProviderKey('telcosopik', businessId) signature Task 58/c
// flags — not built yet, on purpose, per the standing task-splitting
// rule).
export class TelcosOpik {
  constructor(apiKey) {
    if (!apiKey) {
      // Fail closed, not open — same posture getProviderKey()/
      // getProviderBaseUrl() already use for a missing provider
      // secret elsewhere in this codebase (Task 13). Deliberately a
      // plain thrown Error, not providerError() — a missing key is a
      // caller bug (forgot to resolve/pass the business's stored
      // key), not something safe to echo back to an external caller
      // verbatim.
      const err = new Error(
        'TelcosOpik requires an already-resolved api key — pass the ' +
        "business's stored telcos.opik.net key (Task 58/c), not a " +
        'shared env-var secret.'
      );
      err.isConfigError = true; // Task 13: server misconfiguration, not for the client — see routes.js
      throw err;
    }
    this.apiKey = apiKey;
    this.baseUrl = getProviderBaseUrl('telcosopik');
    log('TelcosOpik provider initialized');
  }

  _headers() {
    // Confirmed 2026-09-09 against the live Swagger UI's "Available
    // authorizations" modal: raw `X-API-Key`, no `Bearer` prefix.
    // See docs/guides/02-authentication.md.
    return {
      'X-API-Key': this.apiKey,
      'Content-Type': 'application/json',
    };
  }

  // GET /plans — docs/guides/03-plans-and-networks.md
  async getPlans({ network, category } = {}) {
    const params = new URLSearchParams();
    if (network) params.set('network', network);
    if (category) params.set('category', category);
    const qs = params.toString();

    log(`TelcosOpik Plans Request${qs ? ` (${qs})` : ''}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/plans${qs ? `?${qs}` : ''}`, {
        method: 'GET',
        headers: this._headers(),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.success) {
        throw providerError(responseData.message || 'TelcosOpik plans lookup failed');
      }

      return responseData;
    }, 'telcosopik');

    log(`TelcosOpik Plans Response: ${formatPayload(result)}`);
    return result;
  }

  // GET /wallet — docs/guides/04-wallet-funding.md
  // Returns { balance, total_spent, total_deposited }.
  async getWallet() {
    log('TelcosOpik Wallet Request');

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/wallet`, {
        method: 'GET',
        headers: this._headers(),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.success) {
        throw providerError(responseData.message || 'TelcosOpik wallet lookup failed');
      }

      return responseData;
    }, 'telcosopik');

    log(`TelcosOpik Wallet Response: ${formatPayload(result)}`);
    return result;
  }

  // POST /purchase/data — docs/guides/05-purchasing-data-airtime.md
  // Body: { planId, phoneNumber, network }, forwarded as-is.
  async purchaseData({ planId, phoneNumber, network } = {}) {
    const payload = { planId, phoneNumber, network };

    log(`TelcosOpik Data Purchase Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/purchase/data`, {
        method: 'POST',
        headers: this._headers(),
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.success) {
        throw providerError(responseData.message || 'TelcosOpik data purchase failed');
      }

      return responseData;
    }, 'telcosopik');

    log(`TelcosOpik Data Purchase Response: ${formatPayload(result)}`);
    return result;
  }

  // POST /purchase/airtime — docs/guides/05-purchasing-data-airtime.md
  // Body: { network, phoneNumber, amount }, forwarded as-is.
  async purchaseAirtime({ network, phoneNumber, amount } = {}) {
    const payload = { network, phoneNumber, amount };

    log(`TelcosOpik Airtime Purchase Request: ${formatPayload(payload)}`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/purchase/airtime`, {
        method: 'POST',
        headers: this._headers(),
        body: JSON.stringify(payload),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.success) {
        throw providerError(responseData.message || 'TelcosOpik airtime purchase failed');
      }

      return responseData;
    }, 'telcosopik');

    log(`TelcosOpik Airtime Purchase Response: ${formatPayload(result)}`);
    return result;
  }

  // GET /transactions — docs/guides/06-transactions.md
  // limit/offset forwarded as-is, defaulting to telcos.opik.net's own
  // documented defaults (20/0).
  async getTransactions({ limit = 20, offset = 0 } = {}) {
    const params = new URLSearchParams({ limit: String(limit), offset: String(offset) });

    log(`TelcosOpik Transactions Request (limit=${limit}, offset=${offset})`);

    const result = await handleApiCall(async () => {
      const response = await fetch(`${this.baseUrl}/transactions?${params.toString()}`, {
        method: 'GET',
        headers: this._headers(),
      });

      const responseData = await response.json();

      if (!response.ok || !responseData.success) {
        throw providerError(responseData.message || 'TelcosOpik transactions lookup failed');
      }

      return responseData;
    }, 'telcosopik');

    log(`TelcosOpik Transactions Response: ${formatPayload(result)}`);
    return result;
  }
}

// ==================================================
// 🏗️ ACCOUNT PROVISIONING (Task 58/c, c-1, c-3)
// ==================================================
// Calls telcos.opik.net's own POST /auth/register on a B-Pay
// business's behalf and stores the returned api_key. NOT part of the
// TelcosOpik class above — that class models an already-authenticated
// API client; provisioning is a separate, one-time-per-business
// bootstrap step that produces the credential the class needs.
//
// Trigger (c-1, Stripe-mirrored): callers invoke this at the moment a
// B-Pay business explicitly activates the VTU product (a "Turn on
// Airtime & Data" dashboard action, or the first call to any
// /api/vtu/* route acting as that explicit-activation signal) — never
// at raw B-Pay signup, never lazily mid-purchase. That call site is
// step 5's job (routes.js) — this function only implements the
// provisioning logic itself, synchronously, so its caller can return
// the result (success or failure) to the business immediately, same
// as Stripe's own capability-activation response.
export async function registerTelcosOpikAccount({ email, password, firstName, lastName, companyName } = {}) {
  const baseUrl = getProviderBaseUrl('telcosopik');
  const payload = { email, password, firstName, lastName, companyName };

  log(`TelcosOpik Registration Request: ${formatPayload(payload)}`);

  const result = await handleApiCall(async () => {
    const response = await fetch(`${baseUrl}/auth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    const responseData = await response.json();

    // docs/guides/02-authentication.md: { success, data: { id, email, api_key } }
    if (!response.ok || !responseData.success || !responseData.data?.api_key) {
      throw providerError(responseData.message || 'TelcosOpik account registration failed');
    }

    return responseData;
  }, 'telcosopik');

  log('TelcosOpik Registration Response: (api_key redacted)');
  return result;
}

// Builds the short, non-secret display fragment migration 0012's
// `key_prefix` column exists for (Stripe-dashboard style:
// "sk_live_...a1b2") — never the raw key, never enough of it to
// reconstruct the secret.
function buildKeyPrefix(apiKey) {
  if (!apiKey || apiKey.length < 12) return null;
  return `${apiKey.slice(0, 8)}...${apiKey.slice(-4)}`;
}

// The full check-then-create flow (c-1 trigger + c-3 duplicate-
// registration guard), against the `api_keys` table Task 58's own
// migrations 0010–0013 created. Returns { businessId, provider,
// apiKey, alreadyProvisioned }. The raw apiKey is only ever available
// here — right after registration, before it's handed to Vault for
// encryption — or immediately after this same function's own insert;
// it is NEVER read back out of storage later (Vault's own decrypted
// read path is a separate, deliberate, server-side action reserved
// for the actual telcos.opik.net API call site, not this function).
//
// Errors from every step below propagate to the caller rather than
// being swallowed — this is a provisioning action a business is
// waiting on a synchronous answer for (c-1), not a best-effort
// background write, so it does NOT follow this file's usual
// Supabase-helper "never throws" posture.
export async function provisionTelcosOpikAccount(businessId, registrationDetails = {}) {
  if (!businessId) {
    throw new Error('provisionTelcosOpikAccount requires a businessId');
  }

  // c-3, layer 1: check-then-create. B-Pay's own database is the
  // source of truth for "does this business already have an
  // account" — checked before ever calling telcos.opik.net's
  // registration endpoint, not inferred from its error response.
  const existingRow = await getApiKeyRow(businessId, 'telcosopik');
  if (existingRow) {
    log(`provisionTelcosOpikAccount: business '${businessId}' already has a telcosopik row (id=${existingRow.id}) — skipping registration`);
    return {
      businessId,
      provider: 'telcosopik',
      alreadyProvisioned: true,
      keyPrefix: existingRow.key_prefix || null,
    };
  }

  // Not found — this is a genuine first-time activation. Register a
  // real telcos.opik.net account for this business.
  const registration = await registerTelcosOpikAccount(registrationDetails);
  const rawApiKey = registration.data.api_key;

  // c-2: envelope encryption via Supabase Vault — the raw key is
  // never written to `api_keys` directly, only a Vault reference.
  // See vaultCreateSecret()'s own comment (utils/supabase.js) for the
  // flagged, unverified wrapper-function dependency this call has.
  const vaultSecretId = await vaultCreateSecret(
    rawApiKey,
    `telcosopik:${businessId}`,
    `telcos.opik.net api_key for business ${businessId} (Task 58)`
  );

  // c-3, layer 2: the DB-level `unique (business_id, provider)`
  // constraint (migration 0012) is the actual enforcement point if a
  // concurrent first-activation request for the same business won
  // this same race — insertApiKeyRow() catches that specific case and
  // returns the other request's row instead of throwing.
  const row = await insertApiKeyRow({
    business_id: businessId,
    provider: 'telcosopik',
    vault_secret_id: vaultSecretId,
    key_prefix: buildKeyPrefix(rawApiKey),
  });

  log(`provisionTelcosOpikAccount: provisioned telcosopik account for business '${businessId}' (api_keys row id=${row.id})`);

  return {
    businessId,
    provider: 'telcosopik',
    alreadyProvisioned: false,
    apiKey: rawApiKey,
    keyPrefix: row.key_prefix || null,
  };
}
