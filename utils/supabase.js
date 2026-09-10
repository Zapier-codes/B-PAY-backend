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
export async function recordTransaction({ reference, type, provider, currency, amount, status, provider_reference } = {}) {
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
    // Task 45d: `provider_reference` (migration 0009) is optional and
    // omitted from the insert object entirely when not supplied —
    // Paystack/Korapay/Flutterwave-v3-collection callers never pass
    // it, and there's no reason for those rows to carry an explicit
    // `null` over simply not having the key, since the column itself
    // already defaults to `null` with no `NOT NULL` constraint.
    const row = { reference, type, provider, currency, amount, status };
    if (provider_reference) {
      row.provider_reference = provider_reference;
    }

    const { error } = await client
      .from('transactions')
      .insert(row);

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
// 🧾 BALANCE-TRANSACTION RECORDING (Task 61/b)
// ==================================================
// The write half of the `balance_transactions` table (migrations
// 0014/0015, Task 61/a). Deliberately a sibling of recordTransaction()
// above, not a replacement or a wrapper around it — `transactions`
// answers "what did this one attempt do" (mutable, pending → success/
// failed in place), `balance_transactions` answers "what is the
// complete, ordered ledger of everything that has ever moved a
// balance" (append-only, migration 0014's own header comment). Every
// call site below calls both, not one or the other.
//
// Same "best-effort, non-blocking, never throws" posture as
// recordTransaction() (Task 56/d-3-a), for the identical reason: this
// backend's core job is moving money, and a ledger-write failure must
// never fail or delay the underlying payment/payout/purchase call.
// Callers are expected to call this fire-and-forget, same as
// recordTransaction() itself.
//
// `transaction_id` is deliberately never populated here and always
// lands `null` — recordTransaction()'s own insert (above) discards
// the inserted row's id (no `.select()`), so no call site in this
// file actually has a `transactions.id` to pass in. This is not an
// oversight: migration 0014's own header comment designed `reference`
// as a plain, non-FK correlation column *specifically* so a
// balance_transactions row survives with `transaction_id: null` — the
// same tolerance it built in for the `adjustment` case applies here.
// Wiring a real `transaction_id` through would mean changing
// recordTransaction()'s own contract (adding a `.select()` and a
// return value) — out of scope for this leaf, flagged as a possible
// future tightening rather than done speculatively here.
//
// `business_id` is only ever passed by VTU call sites (Task 58) —
// `/pay`/`/payout` have no business identity available yet
// (`transactions.business_id` itself doesn't exist, migration 0010's
// own still-open note; migration 0014's header comment says the same
// thing about this column). Omitting it (rather than passing
// `undefined` explicitly through every call site) keeps those two
// call sites unchanged by this leaf.
export async function recordBalanceTransaction({ business_id, reference, provider, type, amount, currency, available_on } = {}) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`recordBalanceTransaction skipped — Supabase not available: ${err.message}`, 'warn');
    return;
  }

  try {
    // Mirrors recordTransaction()'s own pattern of omitting an
    // optional column entirely (rather than sending an explicit
    // `null`) when the caller didn't supply it — `business_id` and
    // `available_on` are both nullable columns (migration 0014) with
    // no `NOT NULL` constraint, so leaving them out of the insert
    // object has the same DB-level effect as sending `null` and keeps
    // this object minimal for the common (business_id-less) case.
    const row = { reference, provider, type, amount, currency };
    if (business_id) {
      row.business_id = business_id;
    }
    if (available_on) {
      row.available_on = available_on;
    }

    const { error } = await client
      .from('balance_transactions')
      .insert(row);

    if (error) {
      // A Postgres/PostgREST-level failure (e.g. the CHECK constraint
      // on `type`) — surfaced the same as a thrown error, still
      // swallowed here rather than propagated to the caller, per this
      // function's own "never throws" contract above.
      log(`recordBalanceTransaction insert failed for reference '${reference}': ${error.message}`, 'warn');
    }
  } catch (err) {
    log(`recordBalanceTransaction failed for reference '${reference}': ${err.message}`, 'warn');
  }
}

// ==================================================
// 💰 PER-BUSINESS BALANCE VIEW (Task 61/c)
// ==================================================
// The read/aggregation half of the `balance_transactions` table —
// this is the "concrete prerequisite Task 46's dashboard needs before
// it can show anything real about a business's own funds" that Task
// 61/c's own text names, not the dashboard itself (no UI, no new
// route beyond the thin GET wrapper in routes.js).
//
// Same "never throws, best-effort" posture as recordTransaction()/
// getTransactionByReference() above, for the same reason: a balance
// read failing must become "unknown," not an unhandled exception a
// caller has to guard against separately.
//
// Deliberately client-side aggregation (fetch every row for this
// business, sum in JS), not a Postgres view/RPC — this leaf's own
// scope is "wire the read path," not "design a reconciliation-grade
// aggregation layer" (that discovery pass is explicitly Task 61/d's
// job, still blocked). A real per-provider settlement/statement
// reconciliation could later replace this with something more
// sophisticated (a SQL view, a materialized aggregate, etc.) once
// 61/d's own findings exist — not guessed at here. Fine for the data
// volumes this table will realistically hold before that point.
//
// **Sign convention — a design decision made this leaf, flagged
// rather than silently assumed, since neither `transactions.amount`
// nor `balance_transactions.amount` (migration 0001/0014) documents
// one:** every amount recorded by recordTransaction()/
// recordBalanceTransaction() today is an unsigned magnitude (e.g. a
// ₦500 payout is stored as `amount: 500`, not `-500`) — confirmed by
// reading every current call site in routes.js, not assumed. To turn
// a magnitude into a balance delta, this function applies a fixed
// per-`type` direction: `'payment'` credits (adds to balance),
// `'payout'`/`'fee'`/`'refund'` debit (subtract from balance) — the
// ordinary accounting meaning of each term, not a made-up rule.
// `'adjustment'` is the one exception: since it's a manual correction
// with no fixed direction (Task 63/d's still-open unified-refund
// design aside, no write path exists for it yet either), its stored
// `amount` is treated as *already signed* — a future adjustment
// writer is expected to record a negative value for a downward
// correction, not rely on this function to infer direction. If a
// real signed-amount convention is ever adopted repo-wide instead
// (rather than this function's own per-type direction map), this is
// the one place that assumption would need to change.
//
// Returns an array of `{ currency, available, pending }`, one entry
// per distinct currency this business has ANY ledger activity in — an
// empty array for a business with a real Supabase connection but zero
// rows (a legitimate, distinct state from `null`, which means the
// read itself couldn't be attempted or failed). `available_on: null`
// or a past `available_on` counts as available now, matching
// migration 0014's own "`null` means available immediately" note;
// a future `available_on` counts as pending.
export async function getBusinessBalance(businessId) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`getBusinessBalance skipped — Supabase not available: ${err.message}`, 'warn');
    return null;
  }

  try {
    const { data, error } = await client
      .from('balance_transactions')
      .select('type, amount, currency, available_on')
      .eq('business_id', businessId);

    if (error) {
      log(`getBusinessBalance lookup failed for business '${businessId}': ${error.message}`, 'warn');
      return null;
    }

    const now = Date.now();
    const byCurrency = new Map();

    for (const row of data || []) {
      if (!byCurrency.has(row.currency)) {
        byCurrency.set(row.currency, { currency: row.currency, available: 0, pending: 0 });
      }
      const bucket = byCurrency.get(row.currency);

      // Supabase/PostgREST returns `numeric` columns as strings to
      // avoid float precision loss in transit — `Number()` here
      // matches how every other numeric field already crossing this
      // boundary in this file is handled (no existing precedent for
      // anything more precise, e.g. a decimal library, elsewhere in
      // this repo).
      const magnitude = Number(row.amount);
      const signedAmount = ['payout', 'fee', 'refund'].includes(row.type)
        ? -magnitude
        : magnitude; // 'payment' and 'adjustment' (already-signed) both fall here

      const isAvailable = !row.available_on || new Date(row.available_on).getTime() <= now;
      if (isAvailable) {
        bucket.available += signedAmount;
      } else {
        bucket.pending += signedAmount;
      }
    }

    return Array.from(byCurrency.values());
  } catch (err) {
    log(`getBusinessBalance failed for business '${businessId}': ${err.message}`, 'warn');
    return null;
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
      // Task 45d: `provider_reference` added to this select so GET
      // /verify can resolve JuicyWay's own id from the merchant
      // `reference` — every other existing caller of this function
      // (GET /payout/verify) simply receives one more field in the
      // returned object it doesn't read, same backward-compatible
      // shape this file already keeps for shared helpers.
      .select('currency, provider, type, status, provider_reference')
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

// ==================================================
// 🧭 ROUTING CONFIG — READ (Task 52/e-2d)
// ==================================================
// The read half of the `routing_config` table (migrations 0005/0006).
// Resolves e-2d's own open design question (env var vs. config file
// vs. admin-dashboard toggle) via the Stripe-precedent option the
// product owner directed this task to mirror (Payment Method
// Configurations — a live, Dashboard-toggleable, API-backed object,
// not a deploy): a Supabase-backed row that can be updated directly
// (today, via SQL from the DB-Ops second environment — Task 46's own
// admin dashboard, once it exists, is a UI on top of the same table,
// not a replacement for it), taking effect on the very next request,
// no code deploy required.
//
// Same "never throws, best-effort" posture as every other Supabase
// helper in this file, for the same reason: this is a routing DEFAULT
// lookup on the hot path of every single /pay, /payout, /payout/verify,
// and /banks call — a lookup miss (Supabase unreachable, the table
// not yet migrated on this environment, a domain with no row) must
// fall through to routes.js's own hardcoded `DOMAIN_DEFAULT_PROVIDER`
// safety-net table, exactly the way getTransactionByReference()'s own
// miss already falls through to a hardcoded default for the same
// reason (Task 56/a's accepted trade-off) — it must never turn into a
// 500 or block a payment from resolving a provider at all.
export async function getRoutingDefaultProvider(domain) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`getRoutingDefaultProvider skipped — Supabase not available: ${err.message}`, 'warn');
    return null;
  }

  try {
    const { data, error } = await client
      .from('routing_config')
      .select('default_provider')
      .eq('domain', domain)
      .maybeSingle();

    if (error) {
      log(`getRoutingDefaultProvider lookup failed for domain '${domain}': ${error.message}`, 'warn');
      return null;
    }

    // maybeSingle() resolves data: null (no error) for a genuine miss
    // — no row for this domain yet (e.g. migration 0005 not applied
    // on this environment, or a future classifyDomain() return value
    // this table hasn't been seeded for) — not itself a warning-worthy
    // condition, same as every other by-key lookup miss in this file.
    return data?.default_provider || null;
  } catch (err) {
    log(`getRoutingDefaultProvider failed for domain '${domain}': ${err.message}`, 'warn');
    return null;
  }
}

// ==================================================
// 🧩 CAPABILITY STATUS — READ (Task 52/e-2e)
// ==================================================
// The read half of the `capabilities` table (migrations 0007/0008).
// Resolves e-2e's "not buildable yet, no concrete route exists" gap
// via the Stripe-precedent the product owner directed this task to
// mirror: Stripe's own Capabilities API tracks each capability
// (card_payments, transfers, treasury, ...) as its own independent
// entity with its own status, long before every requirement behind
// it is satisfied — a platform doesn't need a capability's full
// implementation finished to reason about whether it's usable yet.
//
// Same "never throws, best-effort" posture as every other Supabase
// helper in this file. A miss (not configured, table not migrated on
// this environment, or a capability with no seeded row) resolves to
// `null` — the caller (assertCapabilityActive(), routes.js) is what
// decides what a `null`/unknown status means for a request, same
// division of concerns this file already keeps for
// getTransactionByReference()/getCustomerById()/getRoutingDefaultProvider().
export async function getCapabilityStatus(capability) {
  let client;
  try {
    client = getSupabaseClient();
  } catch (err) {
    log(`getCapabilityStatus skipped — Supabase not available: ${err.message}`, 'warn');
    return null;
  }

  try {
    const { data, error } = await client
      .from('capabilities')
      .select('status')
      .eq('capability', capability)
      .maybeSingle();

    if (error) {
      log(`getCapabilityStatus lookup failed for capability '${capability}': ${error.message}`, 'warn');
      return null;
    }

    // maybeSingle() resolves data: null (no error) for a genuine miss
    // — an unrecognized capability name, or one this table hasn't
    // been seeded for yet — not itself a warning-worthy condition.
    return data?.status || null;
  } catch (err) {
    log(`getCapabilityStatus failed for capability '${capability}': ${err.message}`, 'warn');
    return null;
  }
}

// ==================================================
// 🔑 PER-BUSINESS PROVIDER CREDENTIALS — `api_keys` (Task 58/c, c-3)
// ==================================================
// The read half of the `api_keys` table (migrations 0012/0013).
// Deliberately a DIFFERENT posture from every read helper above:
// those are all best-effort/never-throws because they sit on a hot
// payment-routing path where a miss has a safe hardcoded fallback.
// This lookup sits on Task 58/c-3's check-then-create provisioning
// path instead — "does this business already have a
// telcos.opik.net account" is a real precondition for whether
// providers/telcosOpik.js's provisionTelcosOpikAccount() should call
// POST /auth/register at all, so a Supabase-unavailable error here
// must surface to that caller, not silently resolve to "no row found"
// (which would look identical to a genuine first-time activation and
// risk registering a duplicate account). Throws on any failure other
// than a genuine miss.
export async function getApiKeyRow(businessId, provider) {
  const client = getSupabaseClient(); // let a config error propagate — see note above

  const { data, error } = await client
    .from('api_keys')
    .select('id, business_id, provider, vault_secret_id, key_prefix, created_at')
    .eq('business_id', businessId)
    .eq('provider', provider)
    .maybeSingle();

  if (error) {
    throw new Error(`getApiKeyRow lookup failed for business '${businessId}'/provider '${provider}': ${error.message}`);
  }

  // maybeSingle() resolves data: null (no error) for a genuine miss —
  // this business has never provisioned this provider — which IS the
  // "go ahead and provision" signal the c-3 check-then-create flow
  // needs, not an error condition.
  return data || null;
}

// Inserts the new `api_keys` row once provisionTelcosOpikAccount()
// (providers/telcosOpik.js) has a Vault secret reference to store.
// Same "surface real errors, don't swallow" posture as
// getApiKeyRow() above and for the same reason — this is the write
// half of the same provisioning critical path.
//
// Race handling (Task 58/c-3's own second layer, "someone else's
// race won"): migration 0012's `unique (business_id, provider)`
// constraint is the enforcement point if two concurrent first-
// activation requests for the same business both pass the
// check-then-create read above before either has inserted. Postgres
// reports that as error code `23505` (unique_violation) — caught
// here specifically and treated as "provisioning already happened
// concurrently," not a real failure: re-read and return the row the
// other request just inserted rather than throwing.
export async function insertApiKeyRow({ business_id, provider, vault_secret_id, key_prefix } = {}) {
  const client = getSupabaseClient();

  const { data, error } = await client
    .from('api_keys')
    .insert({ business_id, provider, vault_secret_id, key_prefix })
    .select('id, business_id, provider, vault_secret_id, key_prefix, created_at')
    .single();

  if (!error) {
    return data;
  }

  if (error.code === '23505') {
    log(`insertApiKeyRow: unique(business_id, provider) already satisfied for business '${business_id}'/provider '${provider}' — another request won the race, reading its row instead`, 'warn');
    const existing = await getApiKeyRow(business_id, provider);
    if (existing) {
      return existing;
    }
    // Shouldn't happen (the constraint violation implies a row exists)
    // but don't silently return null from a function callers expect
    // a row from — surface it plainly instead of guessing.
    throw new Error(`insertApiKeyRow: unique_violation reported for business '${business_id}'/provider '${provider}' but no row found on re-read`);
  }

  throw new Error(`insertApiKeyRow failed for business '${business_id}'/provider '${provider}': ${error.message}`);
}

// ==================================================
// 🔐 SUPABASE VAULT — secret storage (Task 58/c-2)
// ==================================================
// **Flagged, not silently assumed to work — verify before relying on
// this in production.** `db/SCHEMA.md`'s own Task 58/c-2 note says
// the insert path is `select vault.create_secret(<raw>, <name>,
// <description>)`, which is a direct SQL call against the `vault`
// schema. Supabase's auto-generated REST API (what this file's
// service-role `supabase-js` client actually talks to) only exposes
// schemas explicitly added to the API's schema allowlist — `vault`
// is NOT exposed there by default, specifically because it holds
// decryption-capable functions. Calling `client.rpc('create_secret',
// ...)` the way this function does below only works if a
// `public`-schema `SECURITY DEFINER` wrapper function (e.g. `create
// or replace function public.create_vault_secret(secret text, name
// text, description text) returns uuid ... security definer` calling
// `vault.create_secret` internally) has ALSO been migrated — no such
// wrapper migration exists yet in `db/migrations/`. Until one is
// added and confirmed, treat this function as unverified: the first
// real call is the actual test, and a failure here should surface as
// a real, loud error (see below), not be swallowed.
export async function vaultCreateSecret(rawSecret, name, description) {
  const client = getSupabaseClient();

  const { data, error } = await client.rpc('create_vault_secret', {
    secret: rawSecret,
    name,
    description,
  });

  if (error) {
    throw new Error(
      `vaultCreateSecret failed — this likely means the 'public.create_vault_secret' ` +
      `SECURITY DEFINER wrapper (see this function's own comment) hasn't been migrated ` +
      `yet, not that the raw secret was rejected: ${error.message}`
    );
  }

  if (!data) {
    throw new Error('vaultCreateSecret: no secret id returned from public.create_vault_secret');
  }

  return data; // expected: the new vault.secrets.id (uuid)
}

// Read counterpart to vaultCreateSecret() above — Task 58/c-2's own
// "the real value back requires an explicit `select decrypted_secret
// from vault.decrypted_secrets where id = <vault_secret_id>`" note
// (db/SCHEMA.md). Same unverified-wrapper caveat as the create side:
// `vault.decrypted_secrets` is a `vault`-schema view, not exposed via
// the REST API this file's client talks to, so this assumes a
// second `public`-schema `SECURITY DEFINER` wrapper (e.g. `create or
// replace function public.read_vault_secret(secret_id uuid) returns
// text ... security definer` selecting `decrypted_secret` from
// `vault.decrypted_secrets`) — also not yet migrated. This is the
// function Task 58/e's own future provider call sites (the actual
// `/api/vtu/*` route handlers) will call to turn a business's stored
// `vault_secret_id` back into the real `X-API-Key` value at request
// time; it is NEVER used to display a key anywhere (`key_prefix` is
// the only display-safe value — see migration 0012's own comment) and
// its return value must never be logged.
export async function vaultReadSecret(vaultSecretId) {
  const client = getSupabaseClient();

  const { data, error } = await client.rpc('read_vault_secret', {
    secret_id: vaultSecretId,
  });

  if (error) {
    throw new Error(
      `vaultReadSecret failed — this likely means the 'public.read_vault_secret' ` +
      `SECURITY DEFINER wrapper (see this function's own comment) hasn't been migrated ` +
      `yet, not that the vault_secret_id was invalid: ${error.message}`
    );
  }

  if (!data) {
    throw new Error(`vaultReadSecret: no secret returned for vault_secret_id '${vaultSecretId}'`);
  }

  return data; // the decrypted raw secret string — never log this
}
