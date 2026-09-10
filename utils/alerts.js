// ==================================================
// 🚨 OPS ALERTING (Task 60/e)
// ==================================================
// Task 60/e was blocked on "what does 'notify' mean here" — this file
// is the resolution: a single channel-agnostic `notifyOps()` call site,
// so the *destination* is a deploy-time config value (`ALERT_WEBHOOK_URL`),
// never a hardcoded vendor choice baked into application code.
//
// `ALERT_WEBHOOK_URL` accepts a plain HTTPS endpoint that takes a JSON
// POST — that covers a Slack/Teams/Discord incoming webhook, an
// email-relay endpoint (Postmark/SendGrid/Resend all expose one),
// PagerDuty's Events API v2, or Task 46's future dashboard once it
// exists — the same "one HTTP call to a configured endpoint" shape
// every one of those actually is under the hood. Nothing about the
// alerting logic itself changes when the destination does.
//
// Unconfigured is a real, supported state, not a startup error: until
// `ALERT_WEBHOOK_URL` is set, every alert still lands as a structured
// `error`-level log line, so nothing is silently lost — same "never
// blocks the money-moving path, never throws" posture every other
// helper in utils/supabase.js already follows. This file borrows that
// posture for the same reason: a broken alert channel must never be
// the thing that takes down webhook processing.
import fetch from 'node-fetch';
import { log } from './helpers.js';

export async function notifyOps(subject, details = {}) {
  const payload = {
    subject,
    details,
    timestamp: new Date().toISOString(),
    source: 'b-pay-backend',
  };

  // Always logged, regardless of whether a channel is configured below
  // — this is the guaranteed-delivery floor, per this file's own
  // header note.
  log(`OPS ALERT: ${subject} — ${JSON.stringify(details)}`, 'error');

  const url = process.env.ALERT_WEBHOOK_URL;
  if (!url) {
    log(`notifyOps: ALERT_WEBHOOK_URL not configured — alert logged only, no external channel notified`, 'warn');
    return false;
  }

  try {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    if (!res.ok) {
      log(`notifyOps: ALERT_WEBHOOK_URL responded ${res.status} — alert was still logged above`, 'warn');
      return false;
    }

    return true;
  } catch (err) {
    log(`notifyOps: delivery to ALERT_WEBHOOK_URL failed (${err.message}) — alert was still logged above`, 'warn');
    return false;
  }
}
