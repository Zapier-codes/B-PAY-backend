import crypto from 'crypto';
import fetch from 'node-fetch';
import { log } from './utils/helpers.js';

// ==================================================
// 🌐 KORAPAY WEBHOOK GATEWAY — Task 41
// ==================================================
// Korapay's dashboard has exactly one webhook-URL slot, account-wide.
// The product owner is building multiple multi-tenant apps beyond this
// one, all needing Korapay webhook events — so this backend is the one
// thing Korapay's dashboard ever points at, and this module fans each
// event out to whichever app actually owns it, based on a short prefix
// on the payment `reference` each app generates for itself
// (e.g. `MAVW-<rest>` for mavins-web).
//
// Confirmed by the product owner (see Mavins-web's own handover.md,
// Task 41): B-Pay-backend is the gateway (not a separate repo), and
// mavins-web's prefix is `MAVW`.
//
// ✅ ARCHITECTURE DECISION, CONFIRMED BY THE PRODUCT OWNER — this
// backend gets NO database, ever, by design, not as a temporary gap:
// every app using this as its canonical payment gateway already has
// its own database. This backend's job stops at verifying Korapay's
// signature once and forwarding the event to whichever app owns it —
// durable recording is each app's own responsibility, via its own
// edge function receiving the forward and writing to its own DB (see
// Mavins-web's Task 42: its `korapay-webhook` Edge Function is exactly
// that receiver, verifying this gateway's internal signature and
// recording into its own Supabase). This closes the question Task 12
// left open for the same fork — the answer isn't "add a DB here vs.
// reuse Mavins-web's," it's "no DB here, structurally, by design."
//
// The event store below stays **in-memory only** — not a stopgap
// awaiting a real database, but the correct, final shape for a
// stateless fanout gateway: durable for the life of this process (a
// downstream app being briefly unreachable gets retried correctly
// within that window), intentionally not durable across a
// restart/redeploy. The accepted risk this implies — an event that
// arrives, then this process restarts before both this gateway's own
// retry sweep AND Korapay's own webhook-retry behavior succeed, is
// lost — is a deliberate tradeoff for keeping this backend stateless,
// not an oversight. If that risk ever needs tightening, the fix is
// each app's own edge function polling Korapay's verify endpoint as a
// reconciliation backstop (something already true independent of this
// gateway), not adding persistence here.

// --------------------------------------------------
// Tenant routing table — env-var driven, one entry per downstream app.
// Add a new app by adding its prefix here + its own env vars; no other
// code changes needed for a new tenant.
// --------------------------------------------------
const TENANT_ROUTES = {
  MAVW: {
    name: 'mavins-web',
    forwardUrl: process.env.MAVW_WEBHOOK_URL,
    forwardSecret: process.env.MAVW_WEBHOOK_FORWARD_SECRET,
  },
};

function resolveTenant(reference) {
  if (!reference || typeof reference !== 'string') return null;
  const prefix = reference.split('-')[0]?.toUpperCase();
  const tenant = TENANT_ROUTES[prefix];
  if (!tenant) return null;
  if (!tenant.forwardUrl || !tenant.forwardSecret) {
    log(`Gateway: tenant '${prefix}' matched but has no forwardUrl/forwardSecret configured — check env vars`, 'error');
    return null;
  }
  return { prefix, ...tenant };
}

// --------------------------------------------------
// Event store — in-memory (see limitation note above). Keyed by a
// dedupe key so a Korapay webhook retry (they do retry on non-200)
// doesn't cause a double-forward downstream.
// --------------------------------------------------
const events = new Map();

// Korapay's webhook payload isn't confirmed to carry its own globally
// unique event id anywhere this codebase has seen (only `data.reference`,
// `data.status`, etc. — see routes.js's existing Korapay handler). Per
// Task 41's own spec ("dedupe on korapay event id, or reference + event
// type"), fall back to the always-available pair. If Korapay's docs
// turn out to expose a real event id, prefer that instead — this is a
// deliberate fallback, not a confirmed-best key.
function computeDedupeKey(event, data) {
  return `${event}:${data?.reference}`;
}

const MAX_ATTEMPTS = 5;
// Fixed backoff schedule (ms) by attempt number — simple and readable
// over a computed exponential curve for a table this small.
const BACKOFF_MS = [30_000, 120_000, 600_000, 1_800_000, 3_600_000];

function signForward(payload, secret) {
  return crypto.createHmac('sha256', secret).update(JSON.stringify(payload)).digest('hex');
}

async function attemptForward(record) {
  const { tenant, rawBody } = record;
  try {
    const res = await fetch(tenant.forwardUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Gateway-Signature': signForward(rawBody, tenant.forwardSecret),
        'X-Gateway-Event-Id': record.id,
      },
      body: JSON.stringify(rawBody),
    });

    record.attempts += 1;
    record.lastAttemptAt = Date.now();

    if (res.ok) {
      record.status = 'forwarded';
      record.forwardedAt = Date.now();
      log(`Gateway: forwarded ${record.id} to ${tenant.name} OK (attempt ${record.attempts})`);
      return true;
    }

    record.status = 'failed';
    record.lastError = `HTTP ${res.status}`;
    log(`Gateway: forward of ${record.id} to ${tenant.name} failed — ${record.lastError} (attempt ${record.attempts}/${MAX_ATTEMPTS})`, 'warn');
    return false;
  } catch (err) {
    record.attempts += 1;
    record.lastAttemptAt = Date.now();
    record.status = 'failed';
    record.lastError = err.message;
    log(`Gateway: forward of ${record.id} to ${tenant.name} threw — ${err.message} (attempt ${record.attempts}/${MAX_ATTEMPTS})`, 'warn');
    return false;
  }
}

/**
 * Entry point called from routes.js's Korapay webhook handler, after
 * Korapay's own signature has already been verified there. Records the
 * event, resolves which tenant owns it, and attempts an immediate
 * forward — but the caller should return 200 to Korapay regardless of
 * the forward's outcome (the record + retry sweep is what makes this
 * durable-ish against transient downstream failures, not the response
 * to Korapay). Returns the event record for logging/inspection.
 */
export async function handleGatewayEvent(event, data) {
  const dedupeKey = computeDedupeKey(event, data);

  const existing = events.get(dedupeKey);
  if (existing) {
    log(`Gateway: duplicate event ${dedupeKey} (already ${existing.status}) — Korapay retry, not forwarding again`);
    return existing;
  }

  const tenant = resolveTenant(data?.reference);
  const record = {
    id: dedupeKey,
    reference: data?.reference,
    event,
    tenant,
    rawBody: { event, data },
    status: tenant ? 'received' : 'unroutable',
    attempts: 0,
    lastAttemptAt: null,
    lastError: tenant ? null : `No tenant route for reference '${data?.reference}'`,
    receivedAt: Date.now(),
    forwardedAt: null,
  };

  events.set(dedupeKey, record);

  if (!tenant) {
    log(`Gateway: unroutable event ${dedupeKey} — reference '${data?.reference}' matched no known tenant prefix`, 'error');
    return record;
  }

  await attemptForward(record);
  return record;
}

/**
 * Retry sweep — call on an interval from index.js, same pattern as the
 * existing outbound-IP monitor there. Re-attempts any event still in
 * 'failed' status whose backoff window has elapsed, up to MAX_ATTEMPTS;
 * beyond that it's left at 'failed' permanently (logged loudly once,
 * not on every subsequent sweep) for manual investigation.
 */
export async function retryFailedEvents() {
  const now = Date.now();
  for (const record of events.values()) {
    if (record.status !== 'failed') continue;
    if (record.attempts >= MAX_ATTEMPTS) {
      if (!record.gaveUpLogged) {
        log(`Gateway: giving up on ${record.id} after ${record.attempts} attempts — last error: ${record.lastError}`, 'error');
        record.gaveUpLogged = true;
      }
      continue;
    }
    const backoff = BACKOFF_MS[Math.min(record.attempts - 1, BACKOFF_MS.length - 1)];
    if (now - record.lastAttemptAt < backoff) continue;
    await attemptForward(record);
  }
}

/** For /health or manual debugging — never exposes rawBody contents. */
export function getGatewayStats() {
  const stats = { total: events.size, byStatus: {} };
  for (const record of events.values()) {
    stats.byStatus[record.status] = (stats.byStatus[record.status] || 0) + 1;
  }
  return stats;
}
