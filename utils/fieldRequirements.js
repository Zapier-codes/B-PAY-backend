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
// THIS PART (57/b, split 1 of 4): the registry's shape + accessor
// helpers, and JuicyWay's own entry only — the provider Task 57/a
// already migrated onto the `provider_data` envelope, so it's the
// one with a real gap to close (see routes.js's own `/pay` handler:
// nothing today stops a caller from omitting JuicyWay's required
// nested fields and reaching JuicyWay's API with an incomplete
// payload). Deliberately NOT in this part, left for the remaining
// 3/4 of this split:
//   - Paystack / Korapay / Flutterwave registry entries (their
//     existing inline assertValid* checks in routes.js keep running
//     unchanged until they do)
//   - Wiring this registry into routes.js's `/pay` handler (nothing
//     calls getMissingFields() yet — this file is inert until that
//     lands)
//   - node --check / throwaway-script verification and the
//     handover.md write-up marking this part done
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

  // paystack / korapay / flutterwave: intentionally not added yet —
  // see this part's own note above. Task 57/b's remaining 3/4 covers
  // adding them so `/pay` validation doesn't end up half-migrated
  // (JuicyWay on the registry, everyone else still on inline checks).
};

// Resolves a dot path (e.g. 'provider_data.juicyway.order.identifier')
// against a request body. Returns undefined for any missing segment
// rather than throwing, so callers can treat "not present" uniformly
// regardless of how deep the missing segment is.
function getAtPath(body, path) {
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
function isPresent(value) {
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
