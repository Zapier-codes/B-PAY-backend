-- Migration 0018: index `webhook_events` for Task 60/e's continuous-
-- failure query.
--
-- Migration 0016's own comment deferred this exact index ("no index on
-- status/received_at alone yet ... its real query shape isn't known")
-- until Task 60/e was actually picked up. It now is. The query this
-- serves (utils/supabase.js's `getRecentWebhookOutcomes()`) fetches the
-- most recent N rows for one provider ordered by `received_at`, to
-- count a *consecutive* run of `failed` status back from the newest
-- row — not a plain "how many failed in a time window" count, since a
-- single `processed` row in between should reset the streak the same
-- way a successful Stripe delivery resets its own retry/disable clock.
--
-- Per the Patch Handoff Convention: this file is handed over as a
-- patch. No session applies this migration to a live Supabase project
-- on its own authority — see the DB-Ops Handoff Process in
-- handover.md.

create index if not exists webhook_events_provider_status_idx
  on webhook_events (provider, received_at desc, status);
