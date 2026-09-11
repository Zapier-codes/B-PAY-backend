# Wallet funding

Purchases (`05-purchasing-data-airtime.md`) are debited from a prefunded
wallet, not paid for per-transaction.

**`GET /wallet`** → `{ balance, total_spent, total_deposited }`.

**`POST /wallet/deposit`** → creates one or more virtual bank accounts
to fund that wallet:
```json
{ "accounts": [ { "bankName": "...", "accountNumber": "...", "accountName": "..." } ] }
```
Money sent to that account number should land in the wallet — confirm
the settlement delay and whether `/wallet` needs polling or a webhook
event (`08-webhooks.md`) fires when a deposit clears; not captured in
the source Swagger snapshot.

**Unconfirmed:** what happens on `POST /purchase/*` when the wallet
balance is insufficient — no error-path example was captured. The spec
marks the likely `402` response as `CONFIRM`. Test this directly
against the live server (or its test/sandbox credentials, per this
repo's own testing philosophy in `handover.md` section e) before
building client-side balance-check logic that assumes a specific status
code.
