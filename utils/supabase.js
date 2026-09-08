import { createClient } from '@supabase/supabase-js';
import { log } from './helpers.js';

// ==================================================
// 🗄️ SUPABASE CLIENT (Task 56/d-2)
// ==================================================
// Matches this repo's existing getProviderKey()/getProviderBaseUrl()
// pattern in utils/helpers.js: a getter function, not a client built
// eagerly at import time — so this module can be imported (e.g. by
// index.js at startup) even before SUPABASE_URL/
// SUPABASE_SERVICE_ROLE_KEY are configured, without crashing the
// whole server. The actual client is only constructed, and only
// throws, when a caller asks for it. Cached after the first
// successful call, since createClient() only needs to run once per
// process.
//
// Service-role key, not the anon/public key — this backend talks to
// Supabase as a trusted server, not as an end-user Supabase Auth
// session, matching the service-role posture already decided for
// this table's RLS in Task 56/d-5's own note (permissive policy is
// fine for now because the service-role key bypasses RLS entirely).
// Real values for both env vars are a manual product-owner step in
// Render's dashboard, same class of action as every other secret in
// render.yaml — never hardcoded here.

let cachedClient = null;

export function getSupabaseClient() {
  if (cachedClient) {
    return cachedClient;
  }

  const url = process.env.SUPABASE_URL;
  const serviceRoleKey = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (!url || !serviceRoleKey) {
    // Fail closed, not open — same posture getProviderKey()/
    // getProviderBaseUrl() already use for a missing provider secret.
    // isConfigError is caught by routes.js's clientSafeMessage() so a
    // caller never sees which specific server-side value is missing.
    const err = new Error(
      'Supabase is not configured. Set SUPABASE_URL and SUPABASE_SERVICE_ROLE_KEY.'
    );
    err.isConfigError = true; // Task 13: server misconfiguration, not for the client — see routes.js
    throw err;
  }

  cachedClient = createClient(url, serviceRoleKey, {
    auth: {
      // No end-user session to persist or refresh — this is a
      // server-side service-role client, not a browser/end-user one.
      persistSession: false,
      autoRefreshToken: false,
    },
  });

  return cachedClient;
}

// ==================================================
// 🧾 TRANSACTION RECORDING (Task 56/d-3-a)
// ==================================================
// Shared, best-effort helper — the write half of the `transactions`
// table (migration 0001). Split out of d-3 on its own, per the
// mandatory task-splitting rule: this session builds only the helper
// itself; wiring it into POST /pay (d-3-b) and POST /payout (d-3-c)
// are separate, not-yet-built parts.
//
// "Best-effort, non-blocking" is a restated product decision (Task
// 56/d-3's own text), not this session's own judgment call: this
// backend's core job is moving money, and a logging write failing
// (Supabase unreachable, RLS misconfigured, a bad column value, the
// two env vars still unset on this environment, etc.) must NEVER fail
// or delay the underlying payment/payout call. So this function never
// throws — every failure path is caught and logged, not propagated —
// and callers are expected to call it without awaiting its result on
// the request's critical path (e.g. `recordTransaction(...).catch(() =>
// {})` fire-and-forget, or an `await` placed after the
// payment/payout response has already been decided) once d-3-b/d-3-c
// actually wire it in.
//
// Deliberately thin: one row, one insert, no update/upsert-by-
// reference logic — Task 56/d-4's read path and any future
// status-update need are separate, not-yet-built concerns, not
// silently included here.
export async function recordTransaction({ reference, type, provider, currency, amount, status }) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    // Covers getSupabaseClient()'s own isConfigError (env vars not
    // set on this environment yet) the same way as any other
    // unexpected failure below — either way, this is a log-and-move-on
    // case, never a throw.
    log(`recordTransaction skipped — Supabase not available: ${err.message}`, 'warn');
    return;
  }

  try {
    const { error } = await client
      .from('transactions')
      .insert({ reference, type, provider, currency, amount, status });

    if (error) {
      // A Postgres/PostgREST-level failure (e.g. the CHECK constraint
      // on `status`, or a duplicate `reference` hitting the unique
      // index) — surfaced the same as a thrown error, still swallowed
      // here rather than propagated to the caller.
      log(`recordTransaction insert failed for reference '${reference}': ${error.message}`, 'warn');
    }
  } catch (err) {
    log(`recordTransaction failed for reference '${reference}': ${err.message}`, 'warn');
  }
}

// ==================================================
// 🔎 TRANSACTION LOOKUP (Task 56/d-4)
// ==================================================
// The read half of the `transactions` table (migration 0001) —
// resolves GET /payout/verify's currency gap (Task 52/e-2b-ii) by
// looking a payout's `currency` (and `provider`) up by `reference`,
// instead of that route defaulting to Korapay unconditionally.
//
// Same "never throws, best-effort" posture as recordTransaction()
// (d-3-a), for the same reason stated in Task 56/a's accepted
// trade-off: a lookup miss (no matching row — the write failed at
// `/payout` time, Supabase is unreachable, the env vars aren't set
// on this environment, or the payout simply predates this table)
// falls straight through to the route's own existing default, it
// does not become a 500. That fallback decision belongs to the
// caller (routes.js), not here — this helper only ever returns the
// row's data or `null`, never partial/guessed data and never a
// thrown error.
//
// Deliberately thin, mirroring recordTransaction()'s own scope: a
// single lookup by the unique `reference` index, no caching, no
// update/upsert logic (that's still out of scope, same as d-3-a's
// own note).
export async function getTransactionByReference(reference) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`getTransactionByReference skipped — Supabase not available: ${err.message}`, 'warn');
    return null;
  }

  try {
    const { data, error } = await client
      .from('transactions')
      .select('currency, provider, type, status')
      .eq('reference', reference)
      .maybeSingle();

    if (error) {
      // A Postgres/PostgREST-level failure — surfaced the same as a
      // thrown error below, still resolved to `null` rather than
      // propagated, per this function's own "never throws" contract.
      log(`getTransactionByReference lookup failed for reference '${reference}': ${error.message}`, 'warn');
      return null;
    }

    // `maybeSingle()` resolves `data: null` (no error) on a genuine
    // miss — the normal, expected "payout predates this table, or
    // its own write failed" case from Task 56/a's accepted
    // trade-off, not a warning-worthy condition.
    return data || null;
  } catch (err) {
    log(`getTransactionByReference failed for reference '${reference}': ${err.message}`, 'warn');
    return null;
  }
}

// ==================================================
// 🧑‍💼 CUSTOMER VAULT — READ/WRITE (Task 57/d)
// ==================================================
// The read/write halves of the `customers` table (migrations 0003/
// 0004, Task 57/c). Both mirror recordTransaction()/
// getTransactionByReference()'s own "never throws" posture directly
// above — a vault miss or failure must not become an unhandled
// exception, since Task 57's own resolution order treats "no vaulted
// row" as simply falling through to the next step (a 400 naming the
// missing field, via the existing field-requirements registry), not
// a server error.
//
// The actual resolution-order logic (request field -> vaulted row ->
// name-what's-missing) and the `save_customer` decision live in
// utils/customerVault.js, not here — this file stays scoped to raw
// table access only, same division of concerns Task 56/d already
// established for `transactions` (this file does the query,
// routes.js decides what the result means for the request).

// Reads a single vaulted customer row by its `customers.id`. Returns
// `null` on any miss (no such id, Supabase not configured, a
// Postgres/PostgREST-level error) — never throws, never returns a
// partial/guessed row.
export async function getCustomerById(customerId) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`getCustomerById skipped — Supabase not available: ${err.message}`, 'warn');
    return null;
  }

  try {
    const { data, error } = await client
      .from('customers')
      .select('id, first_name, last_name, phone_number, billing_address, customer_type')
      .eq('id', customerId)
      .maybeSingle();

    if (error) {
      log(`getCustomerById lookup failed for id '${customerId}': ${error.message}`, 'warn');
      return null;
    }

    // maybeSingle() resolves data: null (no error) for a genuine miss
    // — an unrecognized or since-deleted customer_id, not itself a
    // warning-worthy condition (a caller may simply have a stale id).
    return data || null;
  } catch (err) {
    log(`getCustomerById failed for id '${customerId}': ${err.message}`, 'warn');
    return null;
  }
}

// Inserts a new vaulted customer row from whichever of the four
// durable fields (Task 57's own "important nuance" list: name, phone,
// billing address, customer type) are actually present on this call
// — never `email`/`ip_address`, which this table has no column for at
// all (see migration 0003's own comment). Returns the new row's `id`
// on success, or `null` on any failure — a failed save must not block
// or fail the underlying payment, same non-blocking posture
// recordTransaction() already takes for the same reason (this
// backend's core job is moving money; a vault write is a convenience
// on top of that, not a precondition for it). Deliberately no
// upsert-by-id or update path here — every save is a brand-new row,
// matching Task 57's own text ("the response returns the new
// customer_id"); updating an existing vaulted profile is not part of
// this task's scope and is left as its own open item.
export async function saveCustomer({ first_name, last_name, phone_number, billing_address, customer_type } = {}) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`saveCustomer skipped — Supabase not available: ${err.message}`, 'warn');
    return null;
  }

  try {
    const { data, error } = await client
      .from('customers')
      .insert({ first_name, last_name, phone_number, billing_address, customer_type })
      .select('id')
      .single();

    if (error) {
      log(`saveCustomer insert failed: ${error.message}`, 'warn');
      return null;
    }

    return data?.id || null;
  } catch (err) {
    log(`saveCustomer failed: ${err.message}`, 'warn');
    return null;
  }
}
