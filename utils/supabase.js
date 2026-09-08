import { createClient } from '@supabase/supabase-js';

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
