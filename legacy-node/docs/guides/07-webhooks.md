# Webhooks

**`GET /webhooks`** — list registered webhooks for the authenticated
business.

**`POST /webhooks`**
```json
{ "url": "string", "events": ["string"], "secret": "string" }
```
→ same shape back, echoing what was registered.

**Unconfirmed — flagged in `components/schemas.yaml`:**
- The full fixed set of `events` values (e.g. `purchase.success`,
  `deposit.success`) was not visible in the capture — only a
  placeholder `"string"` example. Confirm the real list before
  building event-name-based routing on the receiving end.
- Whether `secret` is client-supplied (as the request shape suggests)
  or server-generated and only echoed back — if client-supplied,
  confirm there's a minimum-strength requirement; if not, that's worth
  raising, since a weak/guessable secret defeats the point of signing
  payloads.
- The signing scheme itself (HMAC-SHA256 over the raw body is the
  common convention, e.g. Paystack/Korapay both use it) isn't stated
  anywhere in the capture — confirm directly, don't assume it matches
  another provider just because the pattern is common.

Once confirmed, update this file and
`components/schemas.yaml#/schemas/Webhook` together — don't let the
human-readable guide and the machine-readable spec disagree.
