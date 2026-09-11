// ==================================================
// 🧭 CUSTOMER VAULT — RESOLUTION ORDER (Task 57/d)
// ==================================================
// Implements the three-step resolution order from Task 57's own
// top-level writeup in handover.md:
//   1. An explicit provider_data.<provider>.customer.<field> in
//      *this* request always wins — lets a customer override one
//      field for a single transaction without touching their stored
//      profile.
//   2. Otherwise, a vaulted `customers` row, if `customer_id` was
//      supplied and a record exists (Task 57/c's table, read here via
//      utils/supabase.js#getCustomerById).
//   3. Still missing after both -> the existing field-requirements
//      registry (Task 57/b, utils/fieldRequirements.js) is what names
//      exactly which field in the 400 — this module does not
//      duplicate that "name the field" job. It only decides what
//      counts as "present" before that check runs.
//
// **Deliberately NOT wired into routes.js's `/pay` handler by this
// part** — that's Task 57/e's job, on top of this. This part only
// builds and verifies the standalone logic, per the standing
// mandatory task-splitting rule (mirrors Task 56/d-3-a building
// recordTransaction() as its own part before d-3-b wired it into
// POST /pay).
//
// Scope, restated from Task 57's own "important nuance": the vault
// only ever covers the `customer` sub-object, never `order` — an
// order's `identifier`/`items`, and any top-level `description`, are
// per-transaction, not per-customer, and are never touched by
// anything in this file.

import { getCustomerById, saveCustomer } from './supabase.js';
import { getAtPath, isPresent, FIELD_REQUIREMENTS } from './fieldRequirements.js';
import { log } from './helpers.js';

// Maps a vaulted `customers` row's own column name (migration 0003)
// to the request-body subfield name it fills under
// provider_data.<provider>.customer.<key>. Every key here is one of
// Task 57's own four durable fields; only `customer_type` differs in
// spelling from its request-shape counterpart (`type`, per Task 57/b's
// juicyway registry entry) — named `customer_type` at the table level
// specifically to avoid clashing with `transactions.type`'s unrelated
// meaning (Task 57/c's own note).
//
// `email` and `ip_address` are deliberately absent — Task 57/c never
// vaults either (email is always supplied fresh per the canonical
// envelope; ip_address is request-time network context, not a durable
// attribute) — so neither is ever fillable from the vault; both must
// always come from the request itself, same as today.
const VAULT_COLUMN_TO_CUSTOMER_KEY = {
  first_name: 'first_name',
  last_name: 'last_name',
  phone_number: 'phone_number',
  billing_address: 'billing_address',
  customer_type: 'type',
};

function vaultColumnForSubfield(subfield) {
  return Object.keys(VAULT_COLUMN_TO_CUSTOMER_KEY).find(
    (column) => VAULT_COLUMN_TO_CUSTOMER_KEY[column] === subfield
  );
}

// Every registry field path (utils/fieldRequirements.js) for this
// provider that lives under provider_data.<providerName>.customer. —
// the only fields the vault is ever allowed to touch, per Task 57's
// own "vault only ever covers the customer sub-object" rule. A
// provider with no such nested paths at all (Paystack, Flutterwave,
// and Korapay's flat top-level `customer.name` today — none of them
// migrated onto a namespaced provider_data.<provider>.customer block
// by Task 57/a) simply gets an empty array back, meaning vault
// resolution is a harmless no-op for it, not an error.
function getVaultableFieldPaths(providerName) {
  const entry = FIELD_REQUIREMENTS[(providerName || '').toLowerCase()];
  if (!entry) return [];

  const prefix = `provider_data.${providerName.toLowerCase()}.customer.`;
  return entry.fields
    .map((field) => field.path)
    .filter((path) => path.startsWith(prefix));
}

// Pure, no-I/O merge step (exported for direct testing): for every
// vaultable field path this provider declares, if the request itself
// didn't already supply it (resolution-order step 1 — explicit always
// wins) and the vaulted row has a non-empty value for the mapped
// column, fill it in on a *clone* of the body. The caller's own
// original body object is never mutated in place.
//
// A JSON round-trip clone (not structuredClone) is used deliberately
// — this repo's request bodies are always plain JSON (no Dates,
// functions, etc.), and a JSON round-trip needs no runtime-version
// assumption across the environments this code actually runs in
// (Render, this sandbox, the product owner's own two checkouts).
export function applyVaultedCustomer(providerName, body, vaultedRow) {
  const paths = getVaultableFieldPaths(providerName);
  if (paths.length === 0 || !vaultedRow) return body;

  const resolved = JSON.parse(JSON.stringify(body || {}));

  for (const path of paths) {
    if (isPresent(getAtPath(resolved, path))) continue; // step 1: explicit request field always wins

    const segments = path.split('.');
    const subfield = segments[segments.length - 1];
    const vaultColumn = vaultColumnForSubfield(subfield);
    if (!vaultColumn) continue; // e.g. ip_address — under the customer prefix but never vaultable, see mapping's own comment

    const vaultValue = vaultedRow[vaultColumn];
    if (!isPresent(vaultValue)) continue; // vaulted row exists but this particular column is itself empty/unset

    let cursor = resolved;
    for (let i = 0; i < segments.length - 1; i++) {
      const seg = segments[i];
      if (typeof cursor[seg] !== 'object' || cursor[seg] === null) {
        cursor[seg] = {};
      }
      cursor = cursor[seg];
    }
    cursor[subfield] = vaultValue;
  }

  return resolved;
}

// Full resolution order, including the I/O steps
// applyVaultedCustomer() itself deliberately stays free of. Returns:
//   - resolvedBody: the body with any vault-filled fields merged in
//     (or the original body, unchanged, if there's no customer_id, no
//     vaultable fields for this provider, or the lookup misses)
//   - customerId: the *new* customer_id if save_customer produced one
//     on this call, else null — never the customer_id the caller
//     passed in (that one is already theirs).
//
// Deliberately does NOT itself compute a final "still missing" list
// or throw a 400 — that remains getMissingFields's (Task 57/b) job,
// to be called by the caller (Task 57/e's /pay wiring) against
// resolvedBody, the same call site and error shape already in
// routes.js today. This function's only responsibility is resolution-
// order steps 1–2 (fill from the vault) and the save_customer side
// effect; step 3 (name what's still missing) is intentionally left to
// the existing, already-verified check rather than duplicated here.
export async function resolveCustomer(providerName, body) {
  const customerId = body?.customer_id;
  let resolvedBody = body;

  if (customerId) {
    const vaultedRow = await getCustomerById(customerId);
    if (vaultedRow) {
      resolvedBody = applyVaultedCustomer(providerName, body, vaultedRow);
    } else {
      log(
        `resolveCustomer: no vaulted row for customer_id '${customerId}' — proceeding with request fields only`,
        'warn'
      );
    }
  }

  let savedCustomerId = null;
  if (body?.save_customer === true) {
    const paths = getVaultableFieldPaths(providerName);
    const customerFields = {};

    for (const path of paths) {
      const subfield = path.split('.').pop();
      const vaultColumn = vaultColumnForSubfield(subfield);
      if (!vaultColumn) continue; // ip_address etc. — never persisted, see mapping's own comment

      const value = getAtPath(resolvedBody, path);
      if (isPresent(value)) customerFields[vaultColumn] = value;
    }

    // Only save if there's actually something vaultable on this
    // request — an all-null insert would produce a customer_id no
    // future call could usefully resolve anything from.
    if (Object.keys(customerFields).length > 0) {
      savedCustomerId = await saveCustomer(customerFields);
    } else {
      log(
        'resolveCustomer: save_customer=true but no vaultable customer fields present on this request — skipping save',
        'warn'
      );
    }
  }

  return { resolvedBody, customerId: savedCustomerId };
}
