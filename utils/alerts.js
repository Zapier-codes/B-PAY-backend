// ==================================================
// 🚨 OPS ALERTING (Task 60/e)
// ==================================================
// Task 60/e was blocked on "what does 'notify' mean here" — this file
// is the resolution: `notifyOps()` fans one alert out over TWO
// independent channels, run concurrently, neither gated on the
// other succeeding:
//
//   1. Generic webhook — `ALERT_WEBHOOK_URL`. A plain HTTPS endpoint
//      that takes a JSON POST. Covers a Slack/Teams/Discord incoming
//      webhook, PagerDuty's Events API v2, or Task 46's future
//      dashboard once it exists.
//   2. Novu — `NOVU_API_KEY` (the account's existing Render secret).
//      Real email delivery, via Novu's own configured workflow, not a
//      vendor this codebase talks SMTP to directly.
//
// Deliberately NOT a fallback chain (try Novu, fall back to webhook
// on failure) — both fire every time, independently. That's the
// actual ask: two unrelated failure domains (a broken Slack webhook
// vs. a Novu outage) don't get to both go dark at once and leave an
// alert unsent. `notifyOps()`'s return value reflects that — see its
// own comment below.
//
// Both channels — and the pre-existing structured log line, which
// fires unconditionally, before either — share the same posture
// every `utils/supabase.js` helper already uses: never throws, never
// blocks the money-moving path. A broken alert channel (either one)
// must never be the thing that takes down webhook processing.
//
// --- Novu integration detail ---
// Novu's trigger endpoint is NOT an open webhook — unlike
// `ALERT_WEBHOOK_URL`, it requires real authentication and a specific
// body shape (confirmed against docs.novu.co this session, not
// guessed):
//
//   POST https://api.novu.co/v1/events/trigger
//   Authorization: ApiKey <NOVU_API_KEY>
//   { "name": "<workflow trigger identifier>",
//     "to": "<subscriberId>" | { "subscriberId": ..., "email": ... },
//     "payload": { ...arbitrary data the workflow's template renders... } }
//
// `name` is a Novu workflow's own trigger identifier — created in the
// Novu dashboard (an email step/template lives inside that workflow,
// not in this code). `NOVU_WORKFLOW_ID` supplies it here; unset, the
// Novu path logs why it's skipped and does nothing, same "unconfigured
// is a supported state" posture as `ALERT_WEBHOOK_URL`.
//
// `to` needs a subscriber. Novu's own docs are explicit that
// pre-creating a real subscriber (`NOVU_SUBSCRIBER_ID`) is the
// supported path, not a guess. If only `NOVU_ALERT_EMAIL` is set
// instead, this code sends `{ subscriberId: <email>, email: <email> }`
// as a best-effort inline recipient — Novu's own docs describe this
// shape inconsistently across versions/SDKs, so this is flagged here
// as an attempt, not a confirmed-working path; if it 400s, that's the
// signal to go create a real subscriber and set `NOVU_SUBSCRIBER_ID`
// instead, not a bug in this file.
//
// --- Task 68 update ---
// The raw `fetch(NOVU_TRIGGER_URL, ...)` call that used to live in
// this file has been pulled out into `utils/novu.js` as
// `triggerWorkflow()` — a generic client the rest of the app can now
// also use for subscriber-facing email (receipts, KYC notices, etc.),
// not just this one ops-alert path. Behavior here is unchanged (same
// env vars, same "unconfigured → log + skip, never throw" posture);
// the only functional addition is `triggerWorkflow()`'s own bounded
// retry-with-backoff on 5xx/network failures and a per-alert
// idempotency key, both new from `utils/novu.js`, neither previously
// present in this file's own inline call.
import fetch from 'node-fetch';
import { log } from './helpers.js';
import { isNovuConfigured, triggerWorkflow, NovuError } from './novu.js';

async function deliverViaWebhook(payload) {
  const url = process.env.ALERT_WEBHOOK_URL;
  if (!url) {
    log(`notifyOps: ALERT_WEBHOOK_URL not configured — webhook channel skipped`, 'warn');
    return false;
  }

  try {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    if (!res.ok) {
      log(`notifyOps: ALERT_WEBHOOK_URL responded ${res.status}`, 'warn');
      return false;
    }
    return true;
  } catch (err) {
    log(`notifyOps: delivery to ALERT_WEBHOOK_URL failed (${err.message})`, 'warn');
    return false;
  }
}

async function deliverViaNovu(payload) {
  const workflowId = process.env.NOVU_WORKFLOW_ID;

  if (!isNovuConfigured() || !workflowId) {
    log(`notifyOps: Novu not configured (need NOVU_API_KEY + NOVU_WORKFLOW_ID) — Novu channel skipped`, 'warn');
    return false;
  }

  const subscriberId = process.env.NOVU_SUBSCRIBER_ID;
  const alertEmail = process.env.NOVU_ALERT_EMAIL;
  let to;
  if (subscriberId) {
    to = subscriberId;
  } else if (alertEmail) {
    // Best-effort inline recipient — see this file's header note on
    // why this shape isn't a confirmed-working path the way a
    // pre-created NOVU_SUBSCRIBER_ID is.
    to = { subscriberId: alertEmail, email: alertEmail };
  } else {
    log(`notifyOps: Novu configured but no recipient set (NOVU_SUBSCRIBER_ID or NOVU_ALERT_EMAIL) — Novu channel skipped`, 'warn');
    return false;
  }

  // A fresh idempotency key per notifyOps() call — NOT deterministic
  // from `payload`, deliberately: two separate ops alerts sharing a
  // subject (e.g. two different "N consecutive failures" alerts for
  // two different providers) must NOT dedup against each other. This
  // key only protects `triggerWorkflow()`'s own internal retries of
  // THIS one call from double-sending — see utils/novu.js's header
  // comment on why a shared key across retries is the whole point.
  const idempotencyKey = `notifyOps-${Date.now()}-${Math.random().toString(36).slice(2)}`;

  try {
    await triggerWorkflow({ workflowId, to, payload }, { idempotencyKey });
    return true;
  } catch (err) {
    if (err instanceof NovuError) {
      log(`notifyOps: Novu trigger responded ${err.statusCode} — ${JSON.stringify(err.body).slice(0, 300)}`, 'warn');
    } else {
      log(`notifyOps: delivery via Novu failed (${err.message})`, 'warn');
    }
    return false;
  }
}

// Fires both channels concurrently and independently — one failing
// never skips or blocks the other. Returns `true` if AT LEAST ONE
// channel delivered (the structured log line always fires regardless
// and is not itself counted here — it's the guaranteed floor, not a
// "channel"). Callers (`routes.js`'s `runWebhookProcessor()`) treat
// this return value as informational only; nothing in the webhook
// path branches on it.
export async function notifyOps(subject, details = {}) {
  const payload = {
    subject,
    details,
    timestamp: new Date().toISOString(),
    source: 'b-pay-backend',
  };

  log(`OPS ALERT: ${subject} — ${JSON.stringify(details)}`, 'error');

  const [webhookDelivered, novuDelivered] = await Promise.all([
    deliverViaWebhook(payload),
    deliverViaNovu(payload),
  ]);

  return webhookDelivered || novuDelivered;
}
