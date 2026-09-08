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
  // guides (Chargebee, Zoho, mctaba.com).
  paystack: ['NGN', 'GHS', 'ZAR', 'KES', 'USD'],
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
      return { unit: 'base', multiplier: 1 };

    case 'juicyway':
      // JuicyWay's CURRENCY LIST was confirmed by the product owner
      // (Task 49/a, see CONFIRMED_PROVIDER_CURRENCIES above) but that
      // is a separate question from the amount-unit rule below — no
      // source audited so far has addressed base units vs. subunits
      // for a JuicyWay charge (stablecoin-denominated or otherwise),
      // so this still throws rather than assuming ×100 or ×1. A
      // silent wrong guess here is a real-money bug, not a cosmetic
      // one.
      throw new Error(
        `getAmountFormat: amount-unit rule for "${provider}" is not yet confirmed — see handover.md Task 49/a note before adding one`
      );

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