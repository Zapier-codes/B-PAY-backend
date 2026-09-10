import 'dotenv/config';
import crypto from 'crypto';

// ==================================================
// 🔒 INTERNAL AUTH — money-moving routes only (Part A of the
// unauthenticated-/payout finding, flagged 2026-08-31)
// ==================================================
// POST /payout had ZERO authentication of any kind until this fix —
// confirmed directly against routes.js before writing anything here,
// not assumed from the flag alone: any request from anywhere on the
// internet could trigger a real Korapay payout to an arbitrary bank
// account, no caller verification whatsoever. This is Part A only —
// the fix for THIS route specifically, the single most severe case
// (outbound money movement, never meant to be reachable by an end
// user's own request at all). Extending the same protection to
// /pay, /verify, /banks, and independently verifying this route's
// amount-unit convention against Korapay's real payout API docs (the
// second half of the original flag, not addressed here) are both
// explicitly left for Part B — see this file's own handover note.
//
// Shared-secret pattern, not per-caller — matches this file's own
// existing MAVW_WEBHOOK_FORWARD_SECRET naming convention (a single
// env var, not a signing scheme) since there's exactly one trusted
// caller today (Mavins-web's own server-side code); can evolve to
// per-caller keys later if a second caller needs distinguishing, not
// over-built for a problem that doesn't exist yet.
export function requireInternalApiKey(req, res, next) {
  const configuredKey = process.env.INTERNAL_API_KEY;

  if (!configuredKey) {
    // Fail closed, not open — same posture this codebase already
    // uses everywhere else a secret might be unconfigured (see
    // korapay-webhook's own signature checks in the sibling Mavins-web
    // repo). An unset key must never be treated as "auth disabled,
    // let everything through."
    log('requireInternalApiKey: INTERNAL_API_KEY is not set — rejecting all requests to this route', 'error');
    return res.status(500).json({ status: 'error', message: 'Server misconfigured' });
  }

  const providedKey = req.headers['x-internal-api-key'];

  if (!providedKey || typeof providedKey !== 'string') {
    return res.status(401).json({ status: 'error', message: 'Missing X-Internal-Api-Key header' });
  }

  // Constant-time comparison — same rigor already established in
  // webhookGateway.js's signature checks, applied here too rather
  // than a plain === (which leaks timing information about how many
  // leading characters matched).
  const configuredBuffer = Buffer.from(configuredKey, 'utf8');
  const providedBuffer = Buffer.from(providedKey, 'utf8');

  const valid =
    configuredBuffer.length === providedBuffer.length &&
    crypto.timingSafeEqual(configuredBuffer, providedBuffer);

  if (!valid) {
    log('requireInternalApiKey: invalid key provided for a protected internal route', 'error');
    return res.status(401).json({ status: 'error', message: 'Invalid X-Internal-Api-Key' });
  }

  next();
}

// ==================================================
// 📝 LOGGING UTILITIES
// ==================================================

export function log(message, level = 'info') {
  const timestamp = new Date().toISOString();
  const prefix = `[${timestamp}]`;
  const levelTag = `[${level.toUpperCase()}]`;
  
  switch (level) {
    case 'error':
      console.error(`${prefix} ${levelTag} ${message}`);
      break;
    case 'warn':
      console.warn(`${prefix} ${levelTag} ${message}`);
      break;
    default:
      console.log(`${prefix} ${levelTag} ${message}`);
  }
}

export function logApiRequest(provider, endpoint, method) {
  log(`📡 ${provider.toUpperCase()} API Request: ${method} ${endpoint}`, 'info');
}

export function logApiResponse(provider, status, success) {
  const emoji = success ? '✅' : '❌';
  log(`${emoji} ${provider.toUpperCase()} API Response: ${status} - ${success ? 'Success' : 'Failed'}`, success ? 'info' : 'error');
}

// ==================================================
// 🔖 REFERENCE GENERATION
// ==================================================

export function generateReference(provider, prefix) {
  const timestamp = Date.now();
  const random = Math.random().toString(36).substring(2, 8).toUpperCase();
  const basePrefix = prefix || provider.toUpperCase();
  return `${basePrefix}-${timestamp}-${random}`;
}

export function isValidReference(reference) {
  return /^[A-Z]+-\d+-[A-Z0-9]+$/.test(reference);
}

// ==================================================
// 🪝 WEBHOOK EVENT DEDUP KEY (Task 60/b, per Task 60/c's discovery)
// ==================================================
// Per Task 60/c's per-provider discovery pass (see handover.md): none
// of Paystack, Korapay, or Juicyway document a dedicated top-level
// event/delivery-id field this codebase can rely on. This is the same
// `${event}:${data.reference}` fallback webhookGateway.js's own
// Korapay-specific computeDedupeKey() already used (Task 41),
// extended here to cover all three providers `routes.js`'s
// `webhookHandlers` verifies. Deliberately uniform across all three —
// Juicyway's `data.transaction_id` is a plausible alternative per
// Task 60/c's own findings but was left unconfirmed, so this
// function does not special-case it; picking the same safe fallback
// for every provider is this leaf's explicit choice, not an oversight.
// Returns `null` if `data.reference` itself is missing — callers
// should treat that as "can't dedupe this one," not throw.
export function computeProviderEventKey(event, data) {
  if (!data?.reference) return null;
  return `${event}:${data.reference}`;
}

// ==================================================
// 🛡️ ERROR HANDLING
// ==================================================

export class ApiError extends Error {
  constructor(provider, statusCode, message, originalError) {
    super(message);
    this.name = 'ApiError';
    this.provider = provider;
    this.statusCode = statusCode;
    this.originalError = originalError;
  }
}

// Task 13 (error-handling review half — rate limiting deliberately
// descoped this session, see handover.md's Task 13 note): each
// provider's processPayment()/verifyTransaction() throws
// `new Error(responseData.message || '...')` when the provider's own
// API returns a failure — that message is provider-authored and
// meant to be shown to the end user (e.g. "Insufficient funds",
// "Invalid account number"). Wrap those specific throws with
// providerError() instead of a bare `new Error(...)` to explicitly
// mark them safe-to-surface. Anything thrown WITHOUT this flag (a
// network failure inside fetch(), a JSON parse failure on a
// non-JSON response, a missing API key, an unsupported provider
// name, etc.) is an internal/operational failure whose raw message
// was never meant for an external caller — handleApiCall below only
// passes the flagged messages through verbatim; everything else gets
// a generic client-facing message while the real detail still goes
// to the server log line right above it (and to `originalError` on
// the thrown ApiError, for anything logging that in future).
export function providerError(message) {
  const err = new Error(message);
  err.isProviderMessage = true;
  return err;
}

export async function handleApiCall(fn, provider = 'unknown') {
  try {
    log(`🔄 Starting API call for ${provider.toUpperCase()}...`, 'info');
    const result = await fn();
    log(`✅ API call completed for ${provider.toUpperCase()}`, 'info');
    return result;
  } catch (err) {
    const errorMessage = err.message || err.toString();
    log(`❌ API Call Error (${provider.toUpperCase()}): ${errorMessage}`, 'error');
    const clientMessage = err.isProviderMessage
      ? errorMessage
      : `Unable to complete request with ${provider} right now. Please try again shortly.`;
    throw new ApiError(provider, err.statusCode || 500, `API request failed: ${clientMessage}`, err);
  }
}

export function validateApiResponse(response, provider) {
  if (!response) {
    throw new Error(`${provider}: Empty response received`);
  }
  
  const successIndicators = [
    response.status === true,
    response.status === 'true',
    response.success === true,
  ];
  
  if (!successIndicators.some(Boolean) && response.status === false) {
    throw new Error(
      `${provider}: ${response.message || response.description || 'Transaction failed'}`
    );
  }
}

// ==================================================
// 🔑 API KEY MANAGEMENT
// ==================================================

export function getProviderKey(provider, type) {
  const providerLower = provider.toLowerCase();
  
  // ✅ Juicyway uses SINGLE key (supports both JUICYWAY_API_KEY and JUICYWAY_PUBLIC_KEY)
  if (providerLower === 'juicyway') {
    const key = process.env.JUICYWAY_API_KEY || process.env.JUICYWAY_PUBLIC_KEY || '';
    if (!key) {
      const err = new Error(`API key not found for ${provider}. Check .env file.`);
      err.isConfigError = true; // Task 13: server misconfiguration, not for the client — see routes.js
      throw err;
    }
    if (key.length < 10) {
      log(`⚠️ Warning: ${provider} key seems too short`, 'warn');
    }
    return key;
  }
  
  // Other providers use public/secret pair
  const keyMap = {
    paystack: {
      public: process.env.PAYSTACK_PUBLIC_KEY || '',
      secret: process.env.PAYSTACK_SECRET_KEY || '',
    },
    korapay: {
      public: process.env.KORAPAY_PUBLIC_KEY || '',
      secret: process.env.KORAPAY_SECRET_KEY || '',
    },
    // Task 52/d-2, part (1) of 3 (v3 method set only this session —
    // v4's OAuth2 client_id/client_secret pair is a deliberately
    // separate, not-yet-built shape, see providers/flutterwave.js's
    // own constructor comment). v3 uses the same static public/secret
    // pair every other provider in this map already does.
    flutterwave: {
      public: process.env.FLW_PUBLIC_KEY || '',
      secret: process.env.FLW_SECRET_KEY || '',
    },
    // Task 52/d-2b — v4's OAuth2 client-credentials pair, deliberately
    // a separate keyMap entry from v3's static public/secret pair
    // above, since the two are structurally different credential
    // shapes on the same underlying Flutterwave merchant account (per
    // handover.md's own v4 Environments note: same dashboard, a
    // toggle reveals v4 credentials instead of v3's). `encryption_key`
    // is a THIRD, distinct secret (per the v4 Encryption discovery
    // note) used client-side only to AES-256-GCM-encrypt card fields
    // before a charge request is built — never sent to the token
    // endpoint or reused as client_secret.
    flutterwave_v4: {
      client_id: process.env.FLW_V4_CLIENT_ID || '',
      client_secret: process.env.FLW_V4_CLIENT_SECRET || '',
      encryption_key: process.env.FLW_V4_ENCRYPTION_KEY || '',
    },
  };

  const providerKeys = keyMap[providerLower];
  
  if (!providerKeys) {
    throw new Error(`Unsupported provider: ${provider}. Supported: paystack, korapay, juicyway, flutterwave`);
  }
  
  const key = providerKeys[type];
  
  if (!key) {
    const err = new Error(`API key not found for ${provider} (${type}). Check .env file.`);
    err.isConfigError = true; // Task 13: server misconfiguration, not for the client — see routes.js
    throw err;
  }
  
  if (key.length < 10) {
    log(`⚠️ Warning: ${provider} ${type} key seems too short`, 'warn');
  }
  
  return key;
}

// ==================================================
// ✅ REQUEST VALIDATION (Task 11)
// ==================================================
// Dependency-free plain-JS validation, per the task's own suggestion
// ("keep this dependency-free... unless the validation logic gets
// unwieldy as plain JS" — it hasn't, so no zod added).

const CURRENCY_CODE_REGEX = /^[A-Za-z]{3}$/;
const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

// Providers whose processPayment() call sites forward customer.email
// (or a top-level email derived from it) straight to the provider's
// API with no fallback default — confirmed by reading each provider
// file directly this session. Paystack and Korapay were already known
// to require one (per the task description); JuicyWay does too
// (providers/juicyway.js forwards `data.customer?.email` with no
// default, same shape as Paystack).
const PROVIDERS_REQUIRING_EMAIL = ['paystack', 'korapay', 'juicyway'];

export function isValidCurrencyCode(currency) {
  return CURRENCY_CODE_REGEX.test(currency || '');
}

export function isValidEmail(email) {
  return typeof email === 'string' && EMAIL_REGEX.test(email);
}

export function providerRequiresEmail(provider) {
  return PROVIDERS_REQUIRING_EMAIL.includes((provider || '').toLowerCase());
}

export function validateProviderKeys() {
  const providers = ['paystack', 'juicyway', 'korapay'];
  const missing = [];
  
  providers.forEach(provider => {
    try {
      getProviderKey(provider, 'secret');
    } catch {
      missing.push(provider);
    }
  });
  
  if (missing.length > 0) {
    log(`⚠️ Missing API keys for: ${missing.join(', ')}`, 'warn');
  } else {
    log('✅ All provider API keys are configured', 'info');
  }
}

// ==================================================
// 📦 PAYLOAD UTILITIES
// ==================================================

export function formatPayload(payload, hideSensitive = true) {
  if (!payload) return 'null';
  
  const sanitized = { ...payload };
  
  if (hideSensitive) {
    const sensitiveFields = ['card', 'cvv', 'pin', 'password', 'secret', 'key', 'token'];
    sensitiveFields.forEach(field => {
      if (sanitized[field]) {
        sanitized[field] = '***REDACTED***';
      }
    });
    
    if (sanitized.customer) {
      sanitized.customer = { ...sanitized.customer };
      if (sanitized.customer.phone) {
        sanitized.customer.phone = sanitizePhone(sanitized.customer.phone);
      }
    }
  }
  
  return JSON.stringify(sanitized, null, 2);
}

export function sanitizePhone(phone) {
  if (!phone || phone.length < 4) return '***';
  return '***' + phone.slice(-4);
}

// ==================================================
// 💱 PER-PROVIDER AMOUNT-UNIT HANDLING (Task 9, partial)
// ==================================================
// Replaces the old blanket toSubUnit()/fromSubUnit() call pattern
// (single ×100 assumption applied to whichever provider happened to
// call it) with a per-provider lookup, since amount-unit rules are NOT
// the same across providers — see handover.md's "Confirmed research
// findings" section for the primary-source evidence behind each case
// below. This is a Korapay-focus partial pass on Task 9: only
// Paystack and Korapay have confirmed rules right now. JuicyWay is
// still unconfirmed, so it throws here rather than silently guessing
// a multiplier — a wrong guess would either overcharge/undercharge by
// 100x or send garbage upstream, so "fail loud" is safer than "fail
// silent" until it gets its own confirmation pass (see Task 49/a).
// The currency-list-expansion half of Task 9 (pulling the real list
// from Mavins-web) is NOT done here — see handover.md's Task 9 note
// for why that's a separate, still-open piece of work.
// Currency lists confirmed against primary sources (see handover.md's
// "Confirmed research findings" section). Only Paystack and Korapay
// have confirmed lists as of Task 10 (Korapay-focus partial) —
// JuicyWay is deliberately omitted rather than guessed; see
// getSupportedCurrencies() below and handover.md's Task 10 note.
//
// Korapay's list cross-checked against Mavins-web's reconciled
// currency source of truth (Task 9b, 2026-08-27): Mavins-web's Task 29
// produced `src/lib/currency/korapayDccCurrency.ts`, which independently
// derives its own Korapay-eligible currency set FROM this same Task 7
// research (that file's own doc comment cites "B-Pay-backend's
// handover.md, Task 7" as its source) — so this is confirming the two
// repos agree, not introducing a second independent source. Both lists
// are identical: NGN, GHS, KES, ZAR, USD, XAF, XOF, EGP, TZS. No values
// changed here as a result — see handover.md's Task 9b note for the
// full cross-check detail, including the separate (and real, but
// out-of-scope-for-this-list) finding that only 8 of Mavins-web's 25
// target countries can actually route through Korapay DCC today.
const CONFIRMED_PROVIDER_CURRENCIES = {
  // paystack.com developer docs, corroborated by multiple integration
  // guides (Chargebee, Zoho, mctaba.com). Task 8d (2026-09-06/08):
  // Paystack's own primary docs ("Supported currency" table) list a
  // SIXTH currency the third-party integration guides above didn't
  // cover — XOF (West African CFA Franc). No amount-unit change
  // needed alongside it: Paystack's own docs state "While there is no
  // subunit for XOF, developers must multiply the amount by 100
  // regardless," so the existing uniform subunit/×100 rule in
  // getAmountFormat('paystack', ...) below already covers it.
  paystack: ['NGN', 'GHS', 'ZAR', 'KES', 'USD', 'XOF'],
  // developers.korapay.com/docs/accept-payments +
  // /docs/payout-via-api (both primary/official). Cross-checked against
  // Mavins-web's reconciled list, Task 9b — see comment above.
  korapay: ['NGN', 'GHS', 'KES', 'ZAR', 'USD', 'XAF', 'XOF', 'EGP', 'TZS'],
  // Task 49/a: three JuicyWay doc pages disagreed with each other
  // (Task 0/a-4's "Three different currency lists" finding) — the
  // product owner has now directly confirmed stablecoin support,
  // resolving the conflict in favor of payments/initialize-payment.md's
  // and cards.md's parameter-docs list over overview.md's narrower
  // NGN/CAD-only list and cards.md's own contradictory 422-error text.
  // This resolves the CURRENCY-LIST question only. The amount-unit rule
  // (base units vs. subunits, specifically for a stablecoin-denominated
  // charge) is a separate, still-unconfirmed question — see
  // getAmountFormat below, which deliberately keeps throwing for
  // 'juicyway' until that gets its own confirmation pass.
  juicyway: ['NGN', 'USD', 'CAD', 'USDT', 'USDC'],
};

// Returns the confirmed supported-currency list for a provider, or null
// if that provider's list hasn't been confirmed against a primary
// source yet. Callers MUST treat null as "can't validate yet" — not
// as "anything goes" — see routes.js's assertCurrencySupported for
// how this is actually enforced.
export function getSupportedCurrencies(provider) {
  return CONFIRMED_PROVIDER_CURRENCIES[(provider || '').toLowerCase()] || null;
}

// Task 52/e-1 — domain-detection logic (currency-based, decided by the
// product owner 2026-09-08: NOT a client-supplied field, and NOT a
// separate country field — the client must not be able to tell this
// backend routes "international" vs "African rails" differently at
// all; the split has to be invisible from the request shape). Every
// currency below is exactly Task 51/b-2's "African rails" domain-covers
// list (NGN/GHS/KES/ZAR/XAF/XOF/EGP/TZS) — kept as its own named
// constant, not inlined, so Task 51's tables and this function can't
// silently drift apart if one is edited without the other.
//
// Known, accepted trade-off (flagged, not solved here): this is a pure
// currency→domain map. A same-currency-different-region case (e.g. a
// USD-denominated charge that's still logically "African rails"
// business) will classify as international, since USD isn't in the
// African-rails list above. The product owner chose this over an
// explicit country/domain field specifically to keep the split
// invisible to the client; revisit only if a real case surfaces where
// that trade-off actually bites.
const AFRICAN_RAILS_CURRENCIES = ['NGN', 'GHS', 'KES', 'ZAR', 'XAF', 'XOF', 'EGP', 'TZS'];

// Classifies a currency into one of Task 51's two rail domains.
// Returns 'african_rails' or 'international' — never anything else,
// so callers (Task 52/e-2's routing rewrite) can switch on the result
// directly without a default/else case of their own.
export function classifyDomain(currency) {
  const currencyUpper = (currency || '').toUpperCase();
  return AFRICAN_RAILS_CURRENCIES.includes(currencyUpper) ? 'african_rails' : 'international';
}

export function getAmountFormat(provider, currency) {
  const providerLower = (provider || '').toLowerCase();
  const currencyUpper = (currency || '').toUpperCase();

  switch (providerLower) {
    case 'paystack': {
      // Confirmed: paystack.com/docs/api/ — "multiplying the base
      // amount by 100" for all 5 supported currencies.
      const supported = getSupportedCurrencies('paystack');
      if (!supported.includes(currencyUpper)) {
        log(`⚠️ Paystack: currency ${currencyUpper} is not in the confirmed-supported list (${supported.join(', ')})`, 'warn');
      }
      return { unit: 'subunit', multiplier: 100 };
    }

    case 'korapay': {
      // Confirmed directly (Task 7, 2026-08-27) against
      // developers.korapay.com/docs/checkout-redirect — base currency
      // unit, no multiplier. Currency list per
      // developers.korapay.com/docs/accept-payments +
      // /docs/payout-via-api.
      const supported = getSupportedCurrencies('korapay');
      if (!supported.includes(currencyUpper)) {
        log(`⚠️ Korapay: currency ${currencyUpper} is not in the confirmed-supported list (${supported.join(', ')})`, 'warn');
      }
      return { unit: 'base', multiplier: 1 };
    }

    case 'flutterwave':
      // Task 52/d-2 (v3 only). Confidence level stated explicitly,
      // weaker than Korapay's/Paystack's own confirmed rules above:
      // no Flutterwave doc page fetched this session (2026-09-08)
      // states "base currency units" in so many words the way
      // Paystack's "multiply by 100" is stated outright — this is
      // inferred from every worked example checked (e.g. a charge of
      // amount: '100' priced as ₦100, not ₦1), which is the same
      // inference class as this file's other "confirmed via examples,
      // not an explicit rule statement" notes elsewhere. One real
      // sandbox call should confirm this before fully trusting it in
      // production. No CONFIRMED_PROVIDER_CURRENCIES entry yet either
      // — a currency-list-specific pass is real, separate follow-up
      // work, not done here; this case intentionally skips the
      // supported-list warning the paystack/korapay cases above do.
      //
      // Task 52/d-2b update: v4's own OpenAPI schema CONFIRMS `amount`
      // as a decimal in major currency units (`12.34`, not `1234`) on
      // both charge endpoints — a stronger-confidence source than v3's
      // inferred-from-examples note above, and coincidentally the same
      // numeric effect (`unit: 'base', multiplier: 1`), so this one
      // case still covers both versions correctly. If a future session
      // ever finds a real base/subunit divergence between v3 and v4
      // for the same currency, this case needs a version parameter —
      // not assumed necessary today.
      return { unit: 'base', multiplier: 1 };

    case 'juicyway': {
      // JuicyWay's CURRENCY LIST was confirmed by the product owner
      // (Task 49/a, see CONFIRMED_PROVIDER_CURRENCIES above). The
      // amount-unit rule below was left throwing for the same reason
      // every other case here demands a real citation before
      // guessing — now resolved, also per Task 49/a:
      // docs.juicyway.com/payments/initialize-payment documents
      // `amount` under "Universal Parameters — required for all
      // payment initializations regardless of the payment method" as
      // "Payment amount in minor units (e.g., cents, kobo) ...
      // Example: 10000 = $100.00 USD" — subunit, ×100, stated to
      // apply across every supported currency (NGN, USD, CAD, USDT,
      // USDC) including the stablecoins, since it's listed as
      // universal rather than per-method. Same confidence bar as the
      // confirmed Korapay/Paystack rules above, not the
      // inferred-from-examples caveat Flutterwave's own case carries.
      const supported = getSupportedCurrencies('juicyway');
      if (!supported.includes(currencyUpper)) {
        log(`⚠️ Juicyway: currency ${currencyUpper} is not in the confirmed-supported list (${supported.join(', ')})`, 'warn');
      }
      return { unit: 'subunit', multiplier: 100 };
    }

    default:
      throw new Error(`getAmountFormat: unsupported provider "${provider}"`);
  }
}

// Convenience wrapper: converts a base-unit input amount into whatever
// unit the given provider actually expects, using getAmountFormat's
// per-provider rule. Provider files should call this instead of the
// old toSubUnit() directly.
export function convertAmountForProvider(amount, provider, currency) {
  const { unit, multiplier } = getAmountFormat(provider, currency);
  return unit === 'subunit' ? Math.round(amount * multiplier) : amount;
}

export function toSubUnit(amount, currency = 'NGN') {
  const subUnitMap = {
    NGN: 100,
    USD: 100,
    GHS: 100,
    KES: 100,
    ZAR: 100,
  };
  
  const multiplier = subUnitMap[currency.toUpperCase()] || 100;
  return Math.round(amount * multiplier);
}

export function fromSubUnit(amount, currency = 'NGN') {
  const subUnitMap = {
    NGN: 100,
    USD: 100,
    GHS: 100,
    KES: 100,
    ZAR: 100,
  };
  
  const divisor = subUnitMap[currency.toUpperCase()] || 100;
  return amount / divisor;
}

// ==================================================
// 🌍 ENVIRONMENT UTILITIES
// ==================================================

export function getEnvironment() {
  return process.env.NODE_ENV || 'development';
}

export function isProduction() {
  return getEnvironment() === 'production';
}

export function getProviderBaseUrl(provider) {
  const env = getEnvironment();
  
  const urlMap = {
    paystack: {
      development: 'https://api.paystack.co',
      production: 'https://api.paystack.co',
    },
    juicyway: {
      development: 'https://api-sandbox.spendjuice.com',
      production: 'https://api.spendjuice.com',
    },
    korapay: {
      development: 'https://api.korapay.com/merchant',
      production: 'https://api.korapay.com/merchant',
    },
    // Task 52/d-2, part (1) of 3 — v3 only. Confirmed
    // (developer.flutterwave.com, multiple endpoint reference pages,
    // 2026-09-08): v3 uses a single host for both test and live mode
    // — which environment a call hits is determined by which secret
    // key (FLWSECK_TEST-... vs FLWSECK-...) is sent, not a different
    // base URL, unlike v4 (see this file's own Flutterwave discovery
    // notes on v4's swapped-base-URL sample-code bug — that problem
    // does not apply to v3, which has no equivalent sandbox/production
    // host split).
    flutterwave: {
      development: 'https://api.flutterwave.com/v3',
      production: 'https://api.flutterwave.com/v3',
    },
    // Task 58/d — confirmed base URL for the VTU integration
    // (`telcos.opik.net`'s own docs-hosting domain vs. its API's real
    // base domain, per Task 45/a's own capture: `telco.` not
    // `telcos.`). No sandbox/production split was seen in that
    // capture (only one server listed), so both environments resolve
    // to the same host, same pattern as Paystack/Korapay above until
    // a real split is confirmed.
    telcosopik: {
      development: 'https://telco.opik.net/api/v1',
      production: 'https://telco.opik.net/api/v1',
    },
    // Task 52/d-2b — v4 DOES have a real sandbox/production host
    // split, unlike v3 above. Per handover.md's own v4 Environments
    // note: sandbox = developersandbox-api..., production =
    // f4bexperience... — confirmed from the page's own PROSE
    // statements, deliberately NOT copying the same page's own
    // "Multi-Environment Integrations" code sample, which has the two
    // hosts swapped relative to its own prose (a confirmed bug in
    // Flutterwave's own docs, not a detail to replicate here).
    flutterwave_v4: {
      development: 'https://developersandbox-api.flutterwave.com',
      production: 'https://f4bexperience.flutterwave.com',
    },
  };
  
  const urls = urlMap[provider.toLowerCase()];
  
  if (!urls) {
    const err = new Error(`No base URL configured for provider: ${provider}`);
    err.isConfigError = true; // Task 13: server misconfiguration, not for the client — see routes.js
    throw err;
  }
  
  return urls[env] || urls.development;
}

// ==================================================
// ⏱️ UTILITIES
// ==================================================

export function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

export async function retryApiCall(fn, maxRetries = 3, provider = 'unknown') {
  let lastError;
  
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      return await fn();
    } catch (err) {
      lastError = err;
      log(`⚠️ Retry ${attempt}/${maxRetries} for ${provider.toUpperCase()}`, 'warn');
      
      if (attempt < maxRetries) {
        const delay = Math.pow(2, attempt) * 1000;
        await sleep(delay);
      }
    }
  }
  
  throw lastError;
}

// ==================================================
// 📊 HEALTH CHECK
// ==================================================

export function getHealthStatus() {
  const providers = ['paystack', 'juicyway', 'korapay'];
  const providerStatus = {};
  
  providers.forEach(provider => {
    try {
      getProviderKey(provider, 'secret');
      providerStatus[provider] = true;
    } catch {
      providerStatus[provider] = false;
    }
  });
  
  return {
    status: 'ok',
    environment: getEnvironment(),
    providers: providerStatus,
    timestamp: new Date().toISOString(),
  };
}

// ==================================================
// 🚀 INITIALIZATION
// ==================================================

export function initializeHelpers() {
  log('🔧 Initializing B-Pay Helpers...', 'info');
  log(`🌍 Environment: ${getEnvironment()}`, 'info');
  validateProviderKeys();
  log('✅ B-Pay Helpers initialized successfully', 'info');
}