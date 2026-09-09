// ==================================================
// 📋 FIELD-REQUIREMENTS REGISTRY (Task 57/b)
// ==================================================
// Data-driven replacement for hardcoding "provider X needs fields
// Y and Z" as scattered inline `if` checks in routes.js. Each
// provider gets ONE entry here declaring what it requires/accepts,
// split across the canonical envelope (amount/currency/reference/
// customer.email — unchanged, provider-agnostic) and its own
// namespaced `provider_data.<provider>` object (Task 57's envelope
// convention). Adding a field or a new provider means adding an
// entry here, not editing conditionals spread across routes.js.
//
// Split 1 of 4 (done): the registry's shape + accessor helpers, and
// JuicyWay's own entry only — the provider Task 57/a already migrated
// onto the `provider_data` envelope, so it's the one with a real gap
// to close (see routes.js's own `/pay` handler: nothing today stops a
// caller from omitting JuicyWay's required nested fields and reaching
// JuicyWay's API with an incomplete payload).
//
// Split 2 of 4 (done): Paystack, Korapay, and Flutterwave registry
// entries, added so `/pay` validation doesn't end up half-migrated
// once (3/4) wired this registry in — JuicyWay alone on the registry
// while everyone else stayed on inline checks would just recreate the
// unmaintainable-branching problem this registry exists to fix. Each
// entry is sourced from that provider's own `processPayment()` (what
// it actually reads off the request body), cross-referenced against
// the doc citations already in that file, not guessed. Flutterwave's
// entry was added even though `providers/flutterwave.js` isn't wired
// into routes.js's `getProvider()` yet.
//
// THIS PART (57/b, split 3 of 4): wired into routes.js's `/pay`
// handler. `getMissingFields(providerName, req.body)` is now called
// right after `getProvider(providerName)` resolves (see that call
// site's own comment in routes.js for exactly why there and not
// earlier/later) — a request missing a required field now gets a
// clean 400 naming it, instead of reaching the provider's API and
// failing there with a less specific error. This is what actually
// makes every entry above (JuicyWay's from split 1, Paystack/Korapay/
// Flutterwave's from split 2) enforce anything; before this part the
// whole registry was inert.
//
// Deliberately NOT in this part, left for the remaining 1/4 of this
// split:
//   - node --check / throwaway-script verification of the wired-in
//     check, end-to-end reasoning about the change, and the
//     handover.md write-up marking the whole 57/b split done (4/4)
//
// Each field descriptor:
//   path      — dot path into the request body, e.g.
//               'provider_data.juicyway.customer.first_name' or
//               'customer.email' for a canonical field
//   required  — true/false
//   label     — human-readable name used in the "missing field" 400
//               message (kept separate from `path` so the message
//               doesn't leak the raw provider_data.<x>.<y> shape at
//               the caller unless that's genuinely the clearest way
//               to say it)
//   note      — optional, short context (e.g. why it's required, a
//               default that applies when it's optional)

export const FIELD_REQUIREMENTS = {
  juicyway: {
    // Confirmed against docs.juicyway.com — see handover.md Task 45b
    // (original audit) and Task 57/a (the provider_data.juicyway
    // migration this registry is closing the validation gap for).
    fields: [
      {
        path: 'customer.email',
        required: true,
        label: 'customer.email',
        note: 'canonical field — already enforced separately by assertValidCustomerEmail in routes.js, listed here too so a single registry lookup returns JuicyWay\'s full requirement set',
      },
      {
        path: 'provider_data.juicyway.description',
        required: true,
        label: 'provider_data.juicyway.description',
      },
      {
        path: 'provider_data.juicyway.payment_method',
        required: false,
        label: 'provider_data.juicyway.payment_method',
        note: "defaults to { type: 'card' } in providers/juicyway.js#processPayment when omitted",
      },
      {
        path: 'provider_data.juicyway.order.identifier',
        required: true,
        label: 'provider_data.juicyway.order.identifier',
      },
      {
        path: 'provider_data.juicyway.order.items',
        required: true,
        label: 'provider_data.juicyway.order.items',
        note: 'documented as an array of { name, type } objects',
      },
      {
        path: 'provider_data.juicyway.customer.first_name',
        required: true,
        label: 'provider_data.juicyway.customer.first_name',
      },
      {
        path: 'provider_data.juicyway.customer.last_name',
        required: true,
        label: 'provider_data.juicyway.customer.last_name',
      },
      {
        path: 'provider_data.juicyway.customer.phone_number',
        required: true,
        label: 'provider_data.juicyway.customer.phone_number',
      },
      {
        path: 'provider_data.juicyway.customer.billing_address',
        required: true,
        label: 'provider_data.juicyway.customer.billing_address',
      },
      {
        path: 'provider_data.juicyway.customer.type',
        required: true,
        label: 'provider_data.juicyway.customer.type',
      },
      {
        path: 'provider_data.juicyway.customer.ip_address',
        required: true,
        label: 'provider_data.juicyway.customer.ip_address',
      },
    ],
  },

  paystack: {
    // providers/paystack.js#processPayment reads only email, amount,
    // currency, reference off the request — no provider_data.paystack
    // namespace exists or is read anywhere. amount/currency are
    // universal fields (see this file's own top note), so the only
    // provider-specific requirement left to record is the email.
    fields: [
      {
        path: 'customer.email',
        required: true,
        label: 'customer.email',
        note: 'canonical field — already enforced separately by assertValidCustomerEmail in routes.js (Paystack is in PROVIDERS_REQUIRING_EMAIL, utils/helpers.js), listed here too so a single registry lookup returns Paystack\'s full requirement set. No provider_data.paystack fields exist — Paystack\'s payload is built from email/amount/currency/reference only.',
      },
    ],
  },

  korapay: {
    // providers/korapay.js#processPayment — customer must be nested
    // (developers.korapay.com/docs/checkout-redirect: "a flat
    // top-level `email` field is rejected", per that file's own
    // comment). payment_currency/settlement_currency/channels/
    // default_channel are the existing top-level optional fields
    // Task 57/a's own writeup explicitly left un-migrated into
    // provider_data (see handover.md, Task 57/a) — recorded here at
    // their current top-level paths, not under provider_data.korapay,
    // to match what routes.js's /pay handler and providers/korapay.js
    // actually read today.
    fields: [
      {
        path: 'customer.email',
        required: true,
        label: 'customer.email',
        note: 'canonical field — already enforced separately by assertValidCustomerEmail in routes.js (Korapay is in PROVIDERS_REQUIRING_EMAIL, utils/helpers.js). Must resolve under customer, not as a flat top-level field — see providers/korapay.js\'s own comment citing developers.korapay.com/docs/checkout-redirect.',
      },
      {
        path: 'customer.name',
        required: false,
        label: 'customer.name',
        note: 'forwarded if present (providers/korapay.js builds customer: { email, name }); not stated as required in the doc excerpt already cited in that file, so left optional here rather than guessed.',
      },
      {
        path: 'payment_currency',
        required: false,
        label: 'payment_currency',
        note: 'Dynamic Currency Conversion (DCC), Korapay-specific — developers.korapay.com/docs/dynamic-currency-conversion. Only meaningful together with settlement_currency (Korapay requires both or neither); this registry records each independently as optional, since a simple required flag can\'t express "required together" — the actual pairing is still enforced only by providers/korapay.js\'s own `if (data.payment_currency && data.settlement_currency)` check, not by getMissingFields() as of this part.',
      },
      {
        path: 'settlement_currency',
        required: false,
        label: 'settlement_currency',
        note: 'see payment_currency\'s note above — same DCC pairing, same "not independently enforced as a pair" caveat.',
      },
      {
        path: 'channels',
        required: false,
        label: 'channels',
        note: 'array of Korapay channel strings (bank_transfer, card, pay_with_bank, mobile_money) — developers.korapay.com/docs/checkout-redirect.',
      },
      {
        path: 'default_channel',
        required: false,
        label: 'default_channel',
        note: 'only meaningful when channels is also supplied — providers/korapay.js drops it otherwise per Korapay\'s own docs. Same "pairing not independently enforced by this registry yet" caveat as payment_currency/settlement_currency above.',
      },
    ],
  },

  flutterwave: {
    // providers/flutterwave.js#processPayment (v3 Standard checkout,
    // Task 52/d-2). Not wired into routes.js's getProvider() yet (see
    // that file's own top comment: "NOT wired into routes.js's
    // getProvider() or ROUTING_RULES — that's Task 52/e's job"), so
    // getMissingFields('flutterwave', ...) has no live caller from
    // /pay until both that routing wiring and this registry's own
    // (3/4) wiring land — added now anyway per this task's own scope
    // ("Existing providers... get registry entries too").
    fields: [
      {
        path: 'customer.email',
        required: true,
        label: 'customer.email',
        note: 'providers/flutterwave.js forwards data.customer?.email straight through with no fallback default — same no-default shape as Paystack/Korapay/JuicyWay\'s email handling (see PROVIDERS_REQUIRING_EMAIL, utils/helpers.js), though Flutterwave is not itself in that list yet since routes.js\'s assertValidCustomerEmail is only reached for providers getProvider() can resolve.',
      },
      {
        path: 'redirect_url',
        required: true,
        label: 'redirect_url',
        note: 'confirmed required per developer.flutterwave.com/docs/flutterwave-standard-1 — providers/flutterwave.js\'s own comment above its payload construction states this directly.',
      },
      {
        path: 'customer.name',
        required: false,
        label: 'customer.name',
        note: 'forwarded if present; not confirmed required by the doc citation already in providers/flutterwave.js.',
      },
      {
        path: 'customer.phone',
        required: false,
        label: 'customer.phone',
        note: 'providers/flutterwave.js reads data.customer?.phone with a data.customer?.phonenumber fallback — either request key works; not confirmed required.',
      },
      {
        path: 'customizations',
        required: false,
        label: 'customizations',
        note: 'optional checkout-page branding, forwarded only if the caller supplies it (providers/flutterwave.js).',
      },
    ],
  },

  // ==================================================
  // Task 58/g — VTU (airtime/data) purchase routes. Keyed by action
  // ('data'/'airtime'), NOT a provider name — unlike every entry
  // above, these two routes don't go through getProvider()/
  // ROUTING_RULES at all (Task 58/e: exactly one provider exists for
  // this domain today, so routes.js calls `new TelcosOpik()`
  // directly). getMissingFields() itself doesn't care either way —
  // it just looks up whatever string key it's given — so reusing it
  // with an action name as the key needs no signature change, just a
  // different call-site convention (routes.js's POST /vtu/data and
  // POST /vtu/airtime pass 'data'/'airtime' literally, not a
  // provider variable). Field lists transcribed directly from Task
  // 58/f (itself transcribed from docs/guides/05-purchasing-data-
  // airtime.md), not re-derived here.
  // ==================================================
  data: {
    // POST /api/vtu/data body, forwarded as-is to telcos.opik.net's
    // POST /purchase/data (providers/telcosOpik.js#purchaseData).
    fields: [
      { path: 'planId', required: true, label: 'planId' },
      { path: 'phoneNumber', required: true, label: 'phoneNumber' },
      { path: 'network', required: true, label: 'network' },
    ],
  },

  airtime: {
    // POST /api/vtu/airtime body, forwarded as-is to telcos.opik.net's
    // POST /purchase/airtime (providers/telcosOpik.js#purchaseAirtime).
    fields: [
      { path: 'network', required: true, label: 'network' },
      { path: 'phoneNumber', required: true, label: 'phoneNumber' },
      { path: 'amount', required: true, label: 'amount' },
    ],
  },
};

// Resolves a dot path (e.g. 'provider_data.juicyway.order.identifier')
// against a request body. Returns undefined for any missing segment
// rather than throwing, so callers can treat "not present" uniformly
// regardless of how deep the missing segment is.
//
// Exported as of Task 57/d: utils/customerVault.js reuses this same
// path-resolution logic against both the request body and a
// candidate merged/resolved body, rather than re-implementing dot-path
// lookup a second time.
export function getAtPath(body, path) {
  return path.split('.').reduce((value, segment) => {
    if (value === null || value === undefined) return undefined;
    return value[segment];
  }, body);
}

// A field counts as "present" if it resolves to anything other than
// undefined, null, or an empty string. Deliberately not stricter than
// that here (e.g. not validating `order.items` is a non-empty array,
// or `billing_address` is a well-formed object) — this registry's job
// per Task 57/b's own description is presence/absence, not full shape
// validation; shape-specific checks stay the provider's own concern
// (or a later, separately-scoped task) same as today.
//
// Exported as of Task 57/d, same reason as getAtPath above —
// utils/customerVault.js needs the identical "is this field actually
// usable" definition when deciding whether a vaulted column, or a
// request field, counts as filled.
export function isPresent(value) {
  if (value === undefined || value === null) return false;
  if (typeof value === 'string' && value.trim() === '') return false;
  return true;
}

// Returns the list of required-but-missing field descriptors for a
// given provider against a given request body. Empty array = nothing
// missing (or the provider has no registry entry yet — see the note
// on FIELD_REQUIREMENTS above; this deliberately does NOT throw for
// an unregistered provider, since most providers don't have an entry
// yet in this part of the split, and the absence of a registry entry
// must not be mistaken for "nothing required").
export function getMissingFields(providerName, body) {
  const entry = FIELD_REQUIREMENTS[(providerName || '').toLowerCase()];
  if (!entry) return [];

  return entry.fields
    .filter((field) => field.required)
    .filter((field) => !isPresent(getAtPath(body, field.path)))
    .map((field) => ({ path: field.path, label: field.label }));
}
