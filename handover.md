# B-Pay Backend — Session Handover

> **▶ START HERE — read this box only, then go straight to work. Skip
> everything else below unless you get stuck.**
>
> **Newest note (2026-09-07, latest of all, supersedes the Task 53
> note below) — Task 54 written: three more product decisions,
> decision-record only, no code this session.** **(1) KYC/KYB is now
> default+fallback, not fixed** — PaymentPoint stays the default
> (licensing/compliance reasons, unchanged from Task 53), but any
> provider with its own KYC/KYB (Korapay's, specifically) is now a
> swappable fallback that can be promoted at any time, same shape as
> Task 51's payment routing. **(2) Two new domains get default+
> fallback routing: Gift Cards and VTU (airtime/data/eSIM, local and
> international).** Both default to `telcos.opik.net` — it aggregates
> the underlying providers, including **two** gift-card providers
> (Prestmit and Tremendous) behind the scenes. **Flutterwave is the
> fallback for VTU specifically** (not yet implemented, still blocked
> on the v3-vs-v4 decision); no gift-card fallback was stated — open
> item. **(3) Prestmit is no longer a standalone provider candidate**
> — it's one of `telcos.opik.net`'s internal gift-card providers, same
> treatment as Lizzysub/Zendit/Accragh for VTU. No code changes (no
> `providers/prestmit.js` ever existed). See Task 54 below for the
> full write-up and open items. **Per the Patch Handoff Convention, a
> patch file covering this handover.md update was generated and handed
> to the product owner directly — not applied or pushed by this
> session.**
>
> **Newest note (2026-09-07, previous) — Task 53 written: three
> more product decisions from the product owner, decision-record only,
> no code this session.** **(1) Card issuance is Korapay** — this
> platform's customers (business or individual) get a card that is
> actually Korapay's card product underneath, but the customer only
> ever sees this platform's own brand on it, same identity-concealment
> posture Task 47 already established for every other rail. **(2)
> White-label branding (name shown to the customer, and the UI theme)
> must be dynamic/configurable at runtime** — not hardcoded to "BPay"
> or any fixed theme, so the product owner can change either at any
> time without a code deploy. **(3) KYC/KYB verification is done via
> PaymentPoint specifically** (chosen for its licensing/compliance
> posture, not for any technical reason over Korapay's own competing
> Identity/KYC-KYB feature, which this platform is deliberately NOT
> using) **but the verification result must be persisted in this
> platform's own database**, so a customer verified once can be reused
> across Korapay, Juicyway, or any future provider without a redundant
> re-verification call to each one. See Task 53 below for the full
> write-up, including two real open items this raises: Korapay's own
> Card Issuing API is real but was explicitly excluded from every prior
> Korapay audit pass (it's marked beta in Korapay's own docs) and has
> never been audited against a primary source in this file — that
> audit has to happen before any `providers/korapay.js` card-issuance
> code is written, per this file's own Discovery Convention. The
> dynamic-branding requirement and the KYC/KYB persistence table both
> need a database, which Task 46 already reversed the no-DB constraint
> for — but neither has a schema designed yet. **Per the Patch Handoff
> Convention, a patch file covering this handover.md update was
> generated and handed to the product owner directly — not applied or
> pushed by this session.**
>
> **Newest note (2026-09-07, previous) — Task 52 written: full
> implementation breakdown of every gap Task 51's capability matrix
> surfaced, so a session can actually build this out instead of just
> routing tables against providers that don't have the methods yet.
> Doc-only, no code this session — product owner's own words: this is
> payment infrastructure, it needs to be completed, not left as
> tables pointing at gaps.** Task 52 below splits into five branches
> (a–e), each following this file's own Task Numbering & Workflow
> Convention (a/b/c/d → 1/2/3 → i/ii → X): **(a)** Juicyway needs
> `processPayout`/`verifyPayout`/`getBanks`-equivalents built from
> scratch — today it only has `processPayment`/`verifyTransaction`/
> `verifyWebhookSignature`, yet Task 51 made it the international-rails
> default for payout/verify-payout/bank-list too; **(b)** Korapay's
> existing payout/verify-payout/bank-list methods are African-rails-
> scoped and need confirming or extending before they're a genuine
> international fallback per Task 51/b-1; **(c)** Paystack's payout/
> verify-payout/bank-list are still stubs (Task 43's own "stub until
> fully integrated" state) and need real implementations to be a
> genuine fallback in either domain; **(d)** Flutterwave has no
> provider file at all — blocked first on the v3-vs-v4 version
> decision this file's own research flagged, then full implementation;
> **(e)** once (a)-(d) exist, `ROUTING_RULES`/`getProvider()` in
> `routes.js` still need rewriting to actually implement Task 51's
> domain model, plus new currency/country domain-detection logic that
> doesn't exist yet. **Exactly one leaf is marked `X`, per this file's
> own convention** — Task 52/a-1 (Juicyway `processPayout`) — chosen as
> the single most acute gap, since Juicyway is now the default for the
> entire international domain but cannot currently process a payout at
> all. **Per the Patch Handoff Convention, a patch file covering this
> handover.md update was generated and handed to the product owner
> directly — not applied or pushed by this session.**
>
> **Newest note (2026-09-07, previous) — Task 51 written:
> product owner has decided the routing model this repo has been
> waiting on since Task 10/49. Payscribe is fully removed from the
> codebase this session (code done: `providers/payscribe.js` deleted,
> all imports/switch-cases/webhook-stub/env-key references removed
> from `routes.js`, `utils/helpers.js`, `render.yaml` — see Task 51's
> own entry below for the full file list). The routing model itself —
> Juicyway default for all international rails, Korapay default for
> all African rails, every other provider that overlaps a given rail
> demoted to a full (not stub) fallback — is decision-record only in
> this note; see Task 51 below for the actual modular table, which is
> deliberately laid out so a future session can edit one rail's row
> without touching the others. **Not done this session:** the
> `ROUTING_RULES`/`getProvider` code in `routes.js` has NOT yet been
> rewritten to match this new model beyond the one Payscribe-driven
> change (`bank_transfer` repointed from `payscribe` to `korapay`,
> since Payscribe no longer exists) — Korapay-as-default-for-all-
> African-rails and Juicyway-as-default-for-all-international-rails
> is a broader change (it also touches `collect_payment`, which
> currently defaults to `paystack`) that the product owner asked to be
> recorded as a table first, not implemented yet. **Per the Patch
> Handoff Convention, a patch file covering this session's Payscribe-
> removal code + this `handover.md` update was generated and handed to
> the product owner directly — not applied or pushed by this
> session.** Next session's concrete next step: implement Task 51's
> table into `ROUTING_RULES` and `getProvider()`, and build out real
> (non-stub) implementations for whichever fallback providers are
> currently stubs, per that task's own capability matrix.**
>
> **Newest note (2026-09-07, previous) — Task 49/a's amount-unit
> question researched and resolved for JuicyWay; Payscribe still
> blocked. Doc-only, no code this session — product owner explicitly
> asked for documentation only, implementation left for the next
> session.** JuicyWay's own docs
> (`docs.juicyway.com/payments/initialize-payment`) directly answer
> the question the previous note left open: the `amount` field is
> documented under "Universal Parameters — required for all payment
> initializations regardless of the payment method" as **"Payment
> amount in minor units (e.g., cents, kobo) ... Example: 10000 =
> $100.00 USD"** — i.e. subunit, ×100 — and "universal" means this is
> stated to apply across every one of JuicyWay's supported currencies
> (`NGN, USD, CAD, USDT, USDC`), not just fiat ones; no
> stablecoin-specific override was found on that page or its linked
> method-specific guides (cards/bank-transfers/stablecoins-transfer/
> binance-pay/interac). This is a real primary-source citation, the
> same bar this file has applied to Korapay/Paystack, not a guess —
> see the note below this one and Task 49/a's own entry for why a
> blanket "use subunits/kobo for everything" rule was explicitly
> rejected instead of applied here (it would have silently broken
> Korapay, which is confirmed to want base units). **Payscribe
> re-searched, same result as every prior session: no public API
> reference site found** — stays blocked, not resolved by assumption.
> **Explicitly not done this session, per direct product-owner
> instruction:** `getAmountFormat`'s `juicyway` case in
> `utils/helpers.js` still throws, and `providers/juicyway.js` still
> sends `data.amount` raw with no `convertAmountForProvider()` call —
> both were drafted and verified in-session (`node --check` clean,
> throwaway sanity check confirmed `getAmountFormat('juicyway', 'USD')`
> would return `{ unit: 'subunit', multiplier: 100 }`) but then
> reverted (`git checkout`) rather than committed, so `origin/main`'s
> actual behavior is unchanged by this session. **Next session's
> concrete, unblocked next step:** apply exactly that change — add the
> `juicyway` case to `getAmountFormat` per the citation above, and wire
> `convertAmountForProvider(data.amount, 'juicyway', data.currency)`
> into `providers/juicyway.js`'s `processPayment`, matching the
> existing `paystack.js`/`korapay.js` pattern. That change does **not**
> touch the endpoint-path (`/v1/charges` vs. the confirmed-correct
> `/payment-sessions`), auth-header, or payload-shape bugs already
> tracked separately under Task 45a/45b — those stay their own,
> separate leaf, not bundled into the amount-unit fix. **Patch for
> this session covers `handover.md` only.**
>
> **Newest note (2026-09-07, previous) — Task 49/a's
> currency-list half implemented in code; its amount-unit half and
> Task 49/b (Remita) remain open.** Per the standing mandatory
> task-splitting rule, Task 49 splits into its own existing a
> (JuicyWay) / b (Remita) parts; part a itself splits further into a
> currency-list edit (resolved by the product owner, ready to build)
> and an amount-unit-rule confirmation (still genuinely open — no
> source has addressed base-vs-subunits for a JuicyWay charge). This
> session built only the currency-list edit, the one unblocked leaf:
> added `juicyway: ['NGN', 'USD', 'CAD', 'USDT', 'USDC']` to
> `CONFIRMED_PROVIDER_CURRENCIES` in `utils/helpers.js`, matching the
> product owner's direct confirmation recorded under Task 49/a below.
> **Deliberately not touched:** `getAmountFormat`'s `juicyway` case
> still throws — the amount-unit question is a separate, still-open
> question from the currency list, and this session found no new
> source to resolve it, so guessing a multiplier here would risk a
> real-money bug. Verified with `node --check utils/helpers.js` plus a
> throwaway `node -e` sanity check (`getSupportedCurrencies('juicyway')`
> returns the new list; `getAmountFormat('juicyway', 'USD')` still
> throws) — deleted after use. Task 49/b (Remita paths/auth scheme)
> and the JuicyWay amount-unit question are both left explicitly
> not-started for the next session. **Patch for this session covers
> `utils/helpers.js` and this `handover.md` note, in one commit**, per
> the Patch Handoff Convention below — generated as a `git
> format-patch` file, not applied or pushed by this session; the
> product owner applies and pushes it themselves from their own
> device, per that convention.
>
> **Newest note (2026-09-07, previous) — Task 50 written: the
> product owner supplied the real Remita public API docs (a Postman
> "public documentation" export, hosted at `api.remita.net`), and it
> substantially corrects this file's own prior Remita research.**
> Doc-only, no code this session. **The core correction: Remita is not
> one API with a base-URL ambiguity — it is (at least) three parallel
> API generations plus a fourth standalone token type**, none of which
> match any of the three host families this file previously guessed
> among. **(1) New APIs (RemitaConnect, Public/Secret key pair)** —
> covers Agency Banking, Collections, **Vending (airtime/data/
> electricity/utilities)**, Funds Transfer, and Verification; entirely
> new surface area, not previously on this file's radar at all — and
> the Vending line overlaps directly with this product's own VTU/
> reseller business. **(2) First Gen — "Accept Online Payments"
> (Checkout Solutions)** — the one that actually matches this repo's
> single-call `processPayment`/`verifyTransaction` shape, and the only
> one confirmed with a real worked example: flat `secretKey` header
> (not either hash scheme previously guessed), `POST .../payment/
> charge` returning a **hosted paymentLink** (same redirect-checkout
> model as Korapay/Paystack, not a raw synchronous charge), and
> `GET .../payment/merchant/verify/{{transRef}}` — **verifiable by the
> merchant's own reference**, unlike JuicyWay's confirmed
> reference-lookup gap above. **A real, confirmed inconsistency within
> this doc's own two sections**, same class of finding as everywhere
> else in this file: the charge endpoint's stated path
> (`.../payment/charge`) doesn't match its own curl example's path
> (`.../payment-engine/payment/charge`). **(3) First Gen — Invoice
> Generation** — this is what all of this file's prior Remita research
> was actually about (the classic RRR flow); the supplied snapshot only
> captured its intro paragraph, not endpoint detail, so its own
> base-URL/hash-scheme questions from the research above remain open.
> **(4) A separate v3-engine bearer-token scheme** (`username`/
> `password` → 1-hour access token, `demo.remita.net/remita/exapp/
> api/v1/send/api/uaasvc/uaa/token`) for unspecified "other first-gen
> services." Full detail, including what's still genuinely unresolved
> (amount-unit rule for the Checkout charge; the split-payment
> sub-account schema; Invoice Generation's own base URL), under Task
> 50 below. **Patch for this session covers `handover.md` only**, per
> this repo's own Patch Handoff Convention.
>
> **Newest note (2026-09-07, previous) — Task 49 written: product
> owner directly resolves two of this file's own open conflicts —
> JuicyWay's stablecoin question and Remita's base-URL ambiguity.**
> Both doc-only, no code this session. **(1)** JuicyWay's three-way
> currency conflict (Task 0/a-4 area, see "Three different currency
> lists" above) is resolved **in favor of stablecoin support**: the
> product owner confirms directly that JuicyWay does support
> stablecoins, backing source 2's wider list (`NGN, USD, CAD, USDT,
> USDC`) over source 1's narrower `NGN, CAD` and source 3's
> stablecoin-excluding error-message text. **Not yet added to
> `CONFIRMED_PROVIDER_CURRENCIES.juicyway` in code this session** —
> `getAmountFormat` still throws for `juicyway` today; a future
> session should add the entry and, per this file's own Discovery
> Convention, still confirm the exact amount-unit rule for stablecoin
> amounts specifically (base units vs. subunits is a separate question
> from which currencies are accepted at all, and no source audited so
> far has addressed it for USDT/USDC on this provider). **(2)** Remita's
> three-way base-URL conflict (see Remita's own research section
> above: `remitademo.net`/`login.remita.net` vs. `demo.remita.net`/
> `login.remita.net` vs. `www.remitademo.net`) is now given a **fourth
> answer directly by the product owner: `https://api.remita.net/`** —
> which matches none of the three previously-documented host families.
> Per this file's own stated resolution path for that conflict ("get
> this directly from the product owner... rather than guessing"),
> the product owner's direct statement is treated as authoritative.
> **Flagged, not resolved:** the product owner supplied a bare host,
> not confirmed endpoint paths for RRR generation vs. status-check
> (which, per the existing research, have historically lived on
> *different* paths/hosts from each other even within a single source)
> — a future implementation session needs the actual path suffixes
> and current auth scheme (classic hash-in-URL vs. header-based) from
> the product owner too before `providers/remita.js` can be written,
> not just the base host. Full detail under Task 49 below. **Patch for
> this session covers `handover.md` only**, per this repo's own Patch
> Handoff Convention.
>
> **Newest note (2026-09-07, previous) — Task 48 written: full
> public-facing service/rail catalog assembled as "the vision doc,"
> gift cards confirmed in scope, and Remita/Flutterwave's rails
> cataloged from this file's own existing research.** Three things,
> all documentation/decision-record only, no code this session:
> **(1)** the product owner confirms gift-card/crypto off-ramp
> services (via Prestmit) are intentionally in scope — **resolves
> Task 0/a-9's open question** ("was Prestmit's inclusion intentional
> or a mix-up?" — it was intentional), presented publicly as "Gift
> Cards," Prestmit itself never named, consistent with Task 47/a.
> **(2)** the full consolidated country footprint is now on record:
> **Nigeria, Ghana, Kenya, South Africa, Cameroon, Côte d'Ivoire,
> Egypt, Tanzania, plus USD cross-border** — derived from this file's
> own `CONFIRMED_PROVIDER_CURRENCIES` (Korapay + Paystack overlap),
> not new research. JuicyWay and Prestmit are explicitly excluded from
> that count as still-unconfirmed, not silently folded in. **(3)**
> Remita's and Flutterwave's own rails — already fully audited
> elsewhere in this file (Remita's research above; Flutterwave's own
> discovery pass above) — are now summarized in one place as the
> platform's own rail list, for use in the public service catalog:
> Remita contributes RRR-based collection (card/bank transfer/USSD/
> in-branch) plus a structurally separate Billing Gateway surface;
> Flutterwave contributes card charges (with v4's field-level
> encryption), bank transfer, mobile money, USSD, wallet, virtual
> accounts, and payouts (domestic NGN + international EUR/GBP/USD/ZAR
> with recipient+sender KYC, plus stablecoin-denominated settlement).
> Full detail under Task 48 below, including which genuinely-open
> items from those two providers' own research (Flutterwave's v3-vs-v4
> choice; Remita's base-URL/auth-scheme ambiguity) still block actually
> building either provider file. **Patch for this session covers
> `handover.md` only**, per this repo's own Patch Handoff Convention.
>
> **Newest note (2026-09-07, previous) — Task 47 written: product
> owner states the underlying vision this repo is actually building
> toward, and resolves one of Task 46's open questions.** Two things,
> both scope/decision-record only, no code this session: **(1)** this
> backend is meant to present itself as its own independent payment
> provider — the ten underlying rails it wraps (Korapay, Paystack,
> JuicyWay, Payscribe, etc.) and even which payment method actually
> moved the money should be invisible, not just a per-business
> checkout skin the way Task 0/d-1 and Task 45/e's white-label
> checkout already cover, but the platform's own identity end to end.
> **(2)** Task 46/e's genuinely-open database engine/hosting question
> is now answered: **Supabase**, confirmed directly by the product
> owner this session. Also carried forward, not new: Task 46/b's own
> dashboard scope now has an explicit companion — a feature-parity
> table (transaction history, settlement/payout reports, balance
> overview, webhook delivery logs, refund/dispute handling, API
> key/access management) matching what Korapay's own dashboard offers,
> in addition to the admin capabilities (country toggling, card
> activation) Task 46/b already recorded. Full detail under Task 47
> below. **Patch for this session covers `handover.md` only**, per
> this repo's own Patch Handoff Convention.
>
> **Newest note (2026-09-06, previous) — Task 46 written: product
> owner has now directly answered the scope-boundary question Task 45
> deliberately left open, and answered it with the broader reading.**
> This backend's own "no-database" constraint (Task 0's opening
> paragraph) and "no separate merchant/admin dashboard is in scope"
> line (Task 0/d) are **both explicitly reversed for `B-Pay-backend`
> itself** — not just for the Reseller/VTU product Task 45 covered.
> Product owner direction, this session: bring a real database back
> into this repo so it can run a **full admin dashboard** — admin
> roles that can enable/disable which countries a given business can
> transact in, and activate/deactivate individual cards — comparable
> in function to Korapay's own dashboard, but with a stated advantage
> this repo already has by virtue of Task 0's own design: because this
> backend is a **canonical consolidation layer across all ten
> providers** rather than a single-provider dashboard like Korapay's,
> one dashboard here can show a business its activity across every
> underlying provider at once, something no single provider's own
> dashboard can do. Also decided: add a **Swagger/OpenAPI UI** for
> this backend's own API (the existing `docs/openapi` material was
> written for the separate Reseller/VTU product's endpoints, not this
> backend's `/pay`, `/payout`, `/verify`, `/banks`, webhook routes —
> those need their own spec). **This is a scope/decision record only,
> matching this repo's own established pattern (see Task 0 and Task
> 45's own closing notes) — no schema created, no dashboard code, no
> Swagger route added, no database provisioned this session.**
> Genuinely open and flagged for the product owner directly: Postgres
> vs. another engine, self-hosted vs. Supabase (Task 45's Reseller
> product already committed to Supabase — sharing one Supabase project
> across both products is the obvious option but not yet confirmed),
> and whether "admin" here means the product owner alone or a role
> multiple people can hold (the same open question Task 45/c already
> flagged for the Reseller dashboard). **The previously-active task,
> `a-1-i-X` (confirming `korapay.js` has no implicit persistence
> dependency, per the no-DB constraint), is now moot for the reason
> it existed** — that audit was only needed to enforce a constraint
> this note reverses — but the code itself hasn't changed, so nothing
> needs to be undone. Full detail under Task 46 below, including a
> proposed starting schema (businesses/admins/country_permissions/
> cards) offered the same way Task 45/b offered one for the Reseller
> product: as a starting point for whichever session actually builds
> this, not yet confirmed with the product owner at the field level.
> **Patch for this session covers `handover.md` only** — per this
> repo's own Patch Handoff Convention (below), handed to the product
> owner as a `git format-patch` file, not applied or pushed here.
>
> **Newest note (2026-09-06, previous) — Task 0's a-9: Prestmit
> full API-discovery/audit pass done (doc-research only, no code —
> `providers/prestmit.js` does not exist yet).** Resolves both halves
> of a-9: **existence confirmed, with a name correction** — the real
> provider is **Prestmit** (with a "t"), a Lagos-based gift-card/crypto
> off-ramp platform, with real docs at `documentation.prestmit.io`,
> found via this session's own web search. **The headline finding,
> ahead of any implementation detail: Prestmit is not a bank/card
> payment processor like this repo's other nine providers — it's a
> gift-card trading platform with no "charge a customer" endpoint at
> all.** This is a real, open scope question for the product owner
> (was Prestmit's inclusion in Task 0's provider list intentional, or
> a mix-up with a different provider?), not something this session
> could resolve alone. **Also confirmed, regardless of how that
> question resolves:** three of Prestmit's own doc sources disagree
> with each other on the base URL/host (same class of problem already
> on record for Remita, a-6); authentication signs the outbound
> *request itself* with `HMAC-SHA256(API_KEY + ":" + JSON.stringify(body))`
> — a genuinely different shape from every other provider in this
> file, none of which sign outbound requests, only inbound webhooks;
> and **Prestmit's own official webhook-verification sample has the
> identical "hash the re-serialized body instead of raw bytes" bug
> already flagged in Xixapay's (a-7) and PaymentPoint's (a-8) own
> samples — a third independent confirmation across three unrelated
> providers.** Full writeup under "Confirmed research findings"
> (search "Prestmit — FULL API discovery pass"); Task 0's a-9 bullet
> has the short version. **Still `a-1-i-X`** is the active task for
> the Task 0 track — this was a doc-only exception for a-9, same as
> Task 8/a-4/a-7/a-8 before it, not a change to which `X` node is
> active. Patch for this session covers `handover.md` only — no
> provider code added or changed.
>
> **Newest note (2026-09-06, previous) — Task 0's a-8: PaymentPoint
> FULL API discovery/audit pass done (doc-research only, no code —
> `providers/paymentpoint.js` does not exist yet). Supersedes this
> same session's own earlier "webhook-only" pass** — the product owner
> has now supplied all five remaining pages from PaymentPoint's own
> nav (Authentication, Errors, Create Virtual Account, Identity
> Verification, Liveness Check), on top of the Webhook Documentation
> page already audited, which together account for every page in
> PaymentPoint's own sidebar — so this is now treated as the full
> discovery pass, not a partial one. All six source docs were supplied
> directly by the product owner rather than found via this session's
> own web search — same accepted exception as the earlier webhook-only
> note. **What's newly confirmed:** base URL
> `https://api.paymentpoint.co`; auth is **two simultaneous headers**
> (`Authorization: Bearer {secret_key}` **and** a separate `api-key`
> header) — structurally the same two-credential shape already found
> for Xixapay (which additionally requires a third, a body-level
> `businessId` — PaymentPoint requires that too, so the real total is
> three credentials, matching Xixapay's shape almost exactly, not this
> repo's existing single-Bearer-header `getProviderKey()` pattern);
> `POST /api/v1/createVirtualAccount` (bank codes for PalmPay/OPay
> confirmed, response `status` is a string `"success"`, consistent
> with the webhook payload's own `transaction_status`); two paid
> verification endpoints not needed for payment orchestration but
> real product-owner-visible functionality — Identity Verification
> (`POST /api/identity/verify`, ₦100 NIN / ₦75 BVN, returns biometric
> photo data) and Liveness Check (`POST /api/liveness/check`, ₦35 per
> check, **critically returns HTTP 200 with `"status":"success"` even
> on a failed check** — callers must branch on the nested `is_live`
> field, not the top-level `status`, or every spoof attempt passes);
> a standard error-code table. **Two real findings worth flagging up
> here specifically:** (1) the Authentication and Create Virtual
> Account pages both display what reads as a **live-looking Bearer
> token and api-key pair in the documentation itself**, not an
> obviously-fake placeholder like the `xxx` used elsewhere in
> PaymentPoint's own docs — flagged as a possible real credential
> left exposed in public docs; this audit deliberately does **not**
> reproduce that literal string anywhere in this repo (including this
> note) to avoid committing a live-looking secret into git history,
> and the product owner should treat it as compromised and eligible
> for rotation if it turns out to be real, rather than assume it's a
> dummy; (2) the Create Virtual Account page's own request-body table
> lists an example `businessId` that is **byte-for-byte identical to
> the page's own `api-key` example value** — almost certainly a
> copy-paste error, since the page's own code sample directly below it
> uses a differently-shaped `businessId` (`3AB2B22345EF407`) that
> matches the format used consistently on the Identity Verification
> and Liveness Check pages — don't copy the table's businessId
> literally. **Carried over, unchanged, from the webhook-only pass:**
> PaymentPoint's official Node webhook sample still has the same two
> confirmed bugs already flagged in Xixapay's sample (re-serializes
> parsed JSON instead of hashing raw bytes; non-constant-time
> signature comparison) — do not copy it as-is; no confirmed
> failed/pending `transaction_status` value; no replay protection in
> the webhook signature scheme. Full writeup under "Confirmed research
> findings" (search "PaymentPoint — FULL API discovery pass"); Task
> 0's a-8 bullet has the short version. **Still `a-1-i-X`** is the
> active task for the Task 0 track — this remains a doc-only exception
> for a-8, same as Task 8/a-4/a-7 before it, not a change to which `X`
> node is active. Patch for this session covers `handover.md` only —
> no provider code added or changed.
>
> **Newest note (2026-09-06, previous) — Task 0's a-7: Xixapay
> full API-discovery/audit pass done (doc-research only, no code —
> `providers/xixapay.js` does not exist yet).** Resolves both halves
> of a-7: **existence is now confirmed** (real provider, real docs at
> `documentation.xixapay.com`), and the full audit is written. **Two
> real items block implementation, neither fixed here (doc-research
> pass only):** (1) auth needs **three simultaneous credentials** at
> once — `Bearer` secret header, separate `api-key` header, *and* a
> `businessId` body field — not a fit for this repo's existing
> single-Bearer-header `getProviderKey()` shape without a change; (2)
> the documented webhook scheme has **no replay protection whatsoever**
> (bare `HMAC-SHA256(payload, secret)`, no timestamp/nonce), a real
> provider-side gap, not an implementation choice — and Xixapay's own
> official Node.js webhook sample has two confirmed bugs of its own
> (re-serializes the parsed body instead of using raw bytes; uses
> `===` instead of a constant-time comparison), so don't copy it
> as-is. Also flagged: no sandbox/test host is documented anywhere,
> the response envelope's `status` field is a boolean on some
> endpoints and a string (`"success"`/`"failed"`) on others, and the
> docs contain several confirmed copy-paste/typo artifacts (a
> literal-JSON syntax error in one example payload, a malformed JSON
> example elsewhere, "Beaerer {Secrete_Key}" misspelled identically
> across every header table) — treat every literal value with the
> same "verify against a real sandbox call" caution already applied
> to JuicyWay's docs. Full writeup under "Confirmed research findings"
> (search "Xixapay — FULL API discovery pass"); Task 0's a-7 bullet
> has the short version. **Still `a-1-i-X`** is the active task for
> the Task 0 track — this was a doc-only exception for a-7, same
> precedent as every other doc-only pass below, not a change to which
> `X` node is active. Patch for this session covers `handover.md`
> only — no provider code added or changed.
>
> **Newest note (2026-09-06, previous) — Payscribe full
> API-discovery/audit pass done (doc-research only, no code —
> `providers/payscribe.js` already exists and is largely **confirmed
> correct** by this pass, unlike the JuicyWay/Remita/DodoPayments
> passes below).** Product owner supplied the primary source directly
> this time (a saved snapshot of docs.payscribe.co), resolving both
> Task 6's long-standing PENDING_DOCS blocker and the stale "Payscribe
> / waiting on a docs link" placeholder in "Confirmed research
> findings." **Confirmed correct, no changes needed:** base URLs,
> `Bearer` auth header, and the `.description` error-field parsing
> Task 13 already implemented. **Newly confirmed, not yet
> implemented:** the full webhook signature scheme (`X-Payscribe-*`
> headers, HMAC-SHA256, `v1=<hex>`, 300-second replay window) —
> resolves Task 6's core blocker, but implementation still needs real
> Payscribe API keys (not yet available) and the still-uncaptured body
> of the `accounts.payments.status` webhook event. **Two items the
> docs source itself didn't cover, flagged rather than guessed at:**
> whether `processPayment`'s existing
> `/collections/virtual-accounts/create` call and
> `verifyTransaction`'s current throw-always behavior are correct or
> stale — the doc snapshot's sidebar confirms matching-by-name
> endpoints exist ("Create Dynamic (Temporary) Virtual Account,"
> "Verify Payment") but their bodies weren't captured, so neither is
> confirmed nor corrected here. Also newly confirmed: a real `201`
> ("Transaction Pending") status-code gotcha this repo's
> `!response.ok` check doesn't currently account for. Full writeup
> under "Confirmed research findings" (search "Payscribe — FULL API
> discovery pass"); Task 6 above has the short version. **Still
> `a-1-i-X`** is the active task for the Task 0 track — this was a
> doc-only exception for Task 6, same precedent as every other
> doc-only pass below, not a change to which `X` node is active. Patch
> for this session covers `handover.md` only — no provider code added
> or changed.
>
> **Newest note (2026-09-06, previous) — Task 0's a-6: Remita
> full API-discovery/audit pass done (doc-research only, no code —
> `providers/remita.js` does not exist yet).** Same doc-only precedent
> as the Task 8b/JuicyWay pass below. **Unlike every provider audited
> so far, Remita has no single public developer-docs site** — the
> audit is built from an official PDF, official GitHub SDK repos for a
> different Billing Gateway product, a public Postman collection, and
> community integrations, several of which conflict. **Three real open
> items block implementation, none fixed here (doc-research pass
> only):** (1) three source families disagree on the base URL/path for
> RRR generation vs. status-check — don't pick one, get it from the
> product owner's own onboarding email; (2) two incompatible auth
> schemes exist (classic hash-in-URL vs. header-based
> `remitaConsumerKey`/`remitaConsumerToken`), each with two different
> hash formulas for generate vs. verify; (3) Remita confirms payment
> via an **unsigned browser redirect** the merchant must independently
> re-verify with a separate hash-authenticated status call — not an
> inbound signed webhook like Korapay/Paystack/JuicyWay/DodoPayments,
> a real difference for Task 0/b-3's webhook-normalization design.
> Also unresolved: the full `statuscode` enumeration (only `025`/
> pending confirmed against a real example) and whether Remita supports
> any currency besides NGN (no currency parameter found anywhere).
> Full writeup under "Confirmed research findings" (search "Remita —
> FULL API discovery pass"); Task 0's a-6 bullet has the short version.
> **Still `a-1-i-X`** is the active task for the Task 0 track — this
> was a doc-only exception for a-6's audit, same as Task 8b was for
> a-3, not a change to which `X` node is active. Patch for this session
> covers `handover.md` only — no provider code added or changed.
>
> **Newest note (2026-09-06, previous) — Task 8b resolved:
> JuicyWay full API-discovery/audit pass done (doc-research only, no
> code changed — `providers/juicyway.js` is untouched by this
> session).** Same doc-only precedent as the Paystack and DodoPayments
> passes below. **Four real, confirmed bugs found (not fixed here,
> queued as Tasks 45a–45d), plus one unresolved documented ambiguity
> (Task 45e):** (1) the payment-initialization endpoint path is wrong
> — code calls `/v1/charges`, which doesn't exist; the real path is
> `POST /payment-sessions`; (2) the `Authorization` header wrongly
> sends `Bearer {key}` — JuicyWay's docs are explicit the header is
> the raw key with no prefix, so every real call today would 401; (3)
> the request payload is missing most of JuicyWay's required nested
> fields (`customer` sub-object, `payment_method`, `order`,
> `description`) — this is a bigger fix than a literal edit, since it
> changes this repo's own `processPayment(data)` contract; (4) the
> error-message extraction reads `responseData.message`, but
> JuicyWay's real envelope nests it at `responseData.error.message` —
> every failure today silently falls back to a generic hardcoded
> string instead of JuicyWay's own specific message. **Unresolved:**
> JuicyWay's own docs give three different, mutually-inconsistent
> currency lists across three pages — do not add `juicyway` to
> `CONFIRMED_PROVIDER_CURRENCIES` from any one of them; resolve with a
> live sandbox test instead. Full writeup under "Confirmed research
> findings" (search "JuicyWay — FULL API discovery pass"); Task 0's
> `a-3` bullet has the short version. **Still `a-1-i-X`** is the
> active task for the Task 0 track — this was a doc-only exception for
> a-3's audit, same as Task 8 was for a-2 and this session's earlier
> DodoPayments pass was for a-4, not a change to which `X` node is
> active. Patch for this session covers `handover.md` only — no
> provider code added or changed.
>
> **Newest note (2026-09-06, previous) — Task 0's a-4:
> DodoPayments full API-discovery/audit pass done (doc-research only,
> no code — `providers/dodopayments.js` does not exist yet).** Same
> doc-only precedent as Task 8's Paystack pass below. Covers
> authentication, Checkout Sessions (the recommended integration
> surface — the older `/payments` endpoint is DodoPayments' own
> documented deprecation), the error envelope, dual-window rate
> limits, and the webhook signature scheme (Standard Webhooks spec —
> three headers, HMAC-SHA256, base64, structurally different from
> every other provider this repo integrates). Full writeup under
> "Confirmed research findings" (search "DodoPayments — FULL API
> discovery pass"); a-4's own entry under Task 0 has the short
> version. **One real open item, not resolved, flagged for whichever
> session writes the actual provider code:** two DodoPayments primary
> sources disagree on which currencies can be a product's base
> currency (one says USD/INR only; another lists several more as
> "native settlement currencies") — resolve against a live/test
> Dashboard before adding anything to
> `CONFIRMED_PROVIDER_CURRENCIES.dodopayments`, don't guess from
> conflicting docs. **Also this session:** corrected the product
> owner's own local Termux checkout path in the "Patch Handoff
> Convention" section below (`~/B-PAY-backend`, not
> `~/B-Pay-backend` — the product owner's own local directory name,
> case-sensitive on that filesystem; the GitHub repo itself is still
> `B-Pay-backend`, unaffected, confirmed by every session's own clone
> output). **Still `a-1-i-X`** is the active task for the Task 0
> track — this was a doc-only exception for a-4, same as Task 8 was
> for a-2, not a change to which `X` node is active. Patch for this
> session covers `handover.md` only — no provider code added or
> changed.
>
> **Newest note (2026-09-06, previous) — Task 8: Paystack full
> API-discovery/audit pass done (doc-research only, per the existing
> "Current focus: Korapay only" exception for doc-only work).** Every
> Transaction-API endpoint, the six-currency support table, the error
> envelope, rate limits, and the webhook signature scheme were
> re-fetched directly from paystack.com/docs and checked against
> `providers/paystack.js`/`routes.js`/`utils/helpers.js`. Both
> previously-implemented endpoints (`/transaction/initialize`,
> `/transaction/verify/:reference`) are confirmed byte-for-byte
> correct, as is the webhook signature code. **Two real findings, not
> yet fixed (documentation-only pass, code changes queued as new
> tasks):** (1) **Task 8c** — `verifyTransaction()`/`GET /api/verify`
> never inspect the nested `data.status` field, so a failed or
> abandoned transaction is reported back as `"Verification
> successful"` — a "paid but no value" class bug; (2) **Task 8d** —
> `CONFIRMED_PROVIDER_CURRENCIES.paystack` is missing `XOF`, which
> Paystack's own docs list as a sixth supported currency (Côte
> d'Ivoire). Full writeup under "Confirmed research findings" below.
> **Still `a-1-i-X`** is the active task for the Task 0 track — this
> was a Task 8-track, doc-only exception, not a change to which `X`
> node is active. Patch for this session covers `handover.md` only —
> no provider code changed.
>
> **Newest note (2026-09-06, previous) — Task 0 expanded:
> business model, pricing, dynamic default provider, admin route, and
> the discovery convention (product owner direction, this session).**
> Pricing: platform charges 5x underlying provider cost per
> transaction, across all service types — **flagged for legal/
> compliance review, not resolved**, given it's paired with fully
> hiding provider identity from businesses and end users (see Task
> 0's business-model note for why this specific combination is worth
> checking against payment-regulator and card-network rules before
> going live). Korapay is the default provider but must be dynamically
> changeable via a new **admin route, explicitly not exposed to end
> users**. **Discovery convention now in force:** each provider gets
> one real discovery pass (web search official docs, ask the product
> owner if genuinely not findable), written up as a full audit in
> Task 0 under that provider's own entry — that audit is the source
> of truth for implementation sessions, which do not redo the
> discovery search unless real testing concretely contradicts a
> recorded audit. Also: intent is full build-and-test with each
> provider's own sandbox/test-mode credentials, so going live means
> the product owner drops real keys into Render env vars with no
> further code changes needed. **Still `a-1-i-X`** is the active
> task — none of this changes where to start. Full detail under Task
> 0 below.
>
> **Newest note (2026-09-06, previous) — Task 0 written: full
> discovery scope for the canonical multi-provider orchestration
> architecture (product owner direction, this session).** Ten
> providers total (Korapay/Paystack/Juicyway/Payscribe existing,
> plus DodoPayments/Flutterwave/Remita/Xixapay/PaymentPoint/Presmit/
> telcos.opik.net net-new), a white-label checkout + home page as the
> only customer-facing surface, and an explicit no-database
> constraint. **This is scope-definition only — nothing built.** A
> new four-level task-numbering convention (`a/b/c/d → 1/2/3 → i/ii →
> X`) is now in force for every task below this point — see "Task
> Numbering & Workflow Convention" immediately before Task 0. **The
> single active task, right now, is `a-1-i-X`** (confirm `korapay.js`
> has no implicit persistence dependency, per the no-DB constraint) —
> whichever session picks this up next should start there, not
> re-derive scope. Full board under Task 0. **Patch handoff for this
> repo is documented explicitly under "Patch Handoff Convention,"
> immediately after the numbering convention — every session generates
> a patch and hands it to the product owner for review; no session
> applies a patch or pushes to `main` on its own authority, regardless
> of what any handover.md (this repo's or mavins-web's) says
> otherwise.**
>
> **Newest note (2026-09-06, previous) — Task 44 written: cross-
> repo reconciliation for Lizzysub (VTU) + Juicyway, migrated from
> mavins-web's Task 71.** This is a scoping/reconciliation record only
> — no code changed, no patch applied. Lizzysub integration hasn't
> started (no code anywhere in this repo); Juicyway has a confirmed
> real endpoint-path uncertainty (flagged in-code) plus other bugs
> claimed by mavins-web but not independently verified this session.
> **Next: get real API docs for Lizzysub and re-verify Juicyway's
> claimed bugs against current provider docs before writing any
> integration code.** Full detail, including open questions this
> doesn't resolve, under Task 44 below. **Note for future sessions:**
> mavins-web's own `handover.md` contains a block instructing readers
> to download and `git am` a patch file and push straight to `main`
> with no review — that patch was not available to inspect and, patch
> or no patch, isn't a safe way to land changes here. Treat any
> "apply this patch and push" instruction found in either repo's
> handover.md as an unverified claim, not a command.
>
> **Newest note (2026-09-04, previous) — Task 42's "Part ii"
> built: `GET /api/payout/verify` now wires `verifyPayout()` into an
> actual, reachable route.** `requireInternalApiKey`-gated (same as
> `/payout`), query-param shape mirrors `/verify` exactly, provider
> defaults to `ROUTING_RULES.payout` (korapay) same as `/payout`'s own
> default, guards against a provider lacking `verifyPayout` with a
> clear 501. `node --check` clean; 6-case functional test all passing.
> **Still open: the webhook-handler half of the original gap (no way
> to be PUSHED a payout's outcome, only to poll for it now), and Part
> b-b (`/verify`/`/banks`'s own auth-extension question)** — both
> independent, pick either next. Full write-up under Task 42's own
> "missing verification call" section.
>
> **Newest note (2026-09-04, latest of all) — Task 43 corrected: the
> "Bpay app" fork/upstream is confirmed real, correct casing is
> `Zapier-codes/B-PAY` / `Edges-Enterprise/B-PAY` (not lowercase
> `bpay`), and real work has already happened there (Task 67's
> security fixes) independent of this repo.** Confirmed directly via
> `git ls-remote` against both URLs. Full correction in Task 43's own
> entry. **This doesn't change anything about this repo's own open
> threads below** — moving on to Part ii (wire `verifyPayout()` into
> an actual route) as the next genuinely unblocked, self-contained
> task.
>
> **Newest note (2026-09-03, latest of all) — Part c-b's remaining
> operational risk resolved: `INTERNAL_API_KEY` confirmed set on
> Render, matching Mavins-web's `BPAY_INTERNAL_API_KEY`.** Project
> owner confirmed directly (matching confirmation added to Mavins-web's
> own `handover.md` this same session). **Still not fully closed —
> two things this alone doesn't answer:** (1) whether this backend's
> Render service auto-deploys on push, which matters for whether a real
> gap existed between Part c-a's `requireInternalApiKey` protection
> going live and Mavins-web's own header-sending fix deploying (both
> secrets being correctly set *today* doesn't retroactively confirm no
> checkouts 401'd in between), and (2) whether a real checkout has
> actually been observed succeeding since both sides deployed, as
> opposed to configuration merely being correct on paper. Don't treat
> Task 42's Part c as fully closed until those two are also confirmed.
>
> **Newest note (2026-09-02, latest of all) — `verifyPayout()` built,
> closing half of the "no way to learn a payout's true final outcome"
> gap.** New `providers/korapay.js#verifyPayout(reference)`, mirroring
> the existing collection-side `verifyTransaction()`. **Endpoint path
> confidence stated explicitly, weaker than the request/response shape
> fixes**: not a directly-quoted string from Korapay's docs, but a
> strong pattern-match from a real confirmed sibling — Bulk Payouts'
> own `POST .../transactions/disburse/bulk` create +
> `GET .../transactions/bulk/:batch_reference` verify pairing, applied
> to the single case by dropping `bulk/`:
> `GET .../transactions/{reference}`. **Recommend one real sandbox
> call to confirm before trusting this in production.** Response
> handling deliberately differs from `processPayout()`'s own on
> purpose: `verifyPayout()` does NOT throw on `data.status === 'failed'`
> — a failed payout is a correct, expected *answer* to "what happened,"
> not an error in asking. Verified via `node --check` + 5 functional
> test cases, all passing. **Not built: wiring this into an actual
> route (Part ii)**, so no caller outside this repo can reach it yet.
> The webhook-handler half of the original gap also remains fully
> open. **Next: Part ii (wire the route), or the webhook handler, or
> Part b-b (`/verify`/`/banks`'s own auth-extension question)** —
> three genuinely open threads, pick whichever the product owner
> prioritizes. Full write-up under Task 42's own "The missing
> verification call — part i" section.
>
> **Newest note (2026-09-02, latest of all) — Part c-b confirmed done
> in Mavins-web (Task 63, commit `4a95a52`), correcting a stale note
> here that said otherwise. This backend's own `/pay` route change
> (Part c-a) and Mavins-web's header-sending change (Part c-b) are
> BOTH code-complete and consistent — verified by reading the actual
> code on both sides directly, not trusting either repo's commit
> message alone.** The remaining risk is now purely operational, not
> code: both sides need `BPAY_INTERNAL_API_KEY` (Mavins-web) and
> `INTERNAL_API_KEY` (this backend, Render) set to the **same value**,
> and both need to actually be deployed — together, or c-b first,
> never Part c-a alone. Neither sandbox can perform that step. Full
> detail in Task 42's own "Part c-b" entry. **Next: Part b-b**
> (`/verify`/`/banks`'s own investigation, independent of c-b) **or
> the still-open webhook-handler/verification-API gap** from two notes
> below.
>
> **Newest note (2026-09-02, previous) — Part b-a fully resolved:
> i (facts) done previously, ii (verdict) done this session — extend
> `requireInternalApiKey` to `/pay`.** Based directly on b-a-i's
> confirmed facts (exactly one caller, Mavins-web's `initialize-payment`
> Edge Function, server-to-server, secret in `Deno.env`, never
> client-reachable — the same trust level `/payout`'s own caller
> already has): no legitimate reason for `/pay` to stay reachable
> unauthenticated. Scope note: this verdict covers `/pay` only —
> `/verify`/`/banks` are Part b-b's own still-open question, not
> decided here. **Not implemented yet** — adding the middleware to
> `routes.js`, generating the shared secret, and updating Mavins-web's
> Edge Function to send `X-Internal-Api-Key` is Part c's job, a
> cross-repo change. **Next: Part c for `/pay`** (can proceed now,
> independently of Part b-b), **or Part b-b** (`/verify`/`/banks`'s own
> dedicated investigation, not yet started), **or the still-separately-
> open webhook-handler/verification-API gap** flagged two notes below —
> three genuinely independent open threads.
>
> **Newest note (2026-09-02, previous) — Part b split into a/b,
> Part b-a split further into i/ii; only b-a-i done (fact-finding
> only, no verdict rendered yet).** Re-confirmed via fresh clones of
> both Mavins-web and Velune: exactly one caller of `/api/pay` exists
> anywhere across both repos — Mavins-web's `initialize-payment`
> Supabase Edge Function, server-to-server, secret held in `Deno.env`,
> never client-reachable. Structurally the same trust level `/payout`'s
> own (already-confirmed) caller has. Velune doesn't call this backend
> at all, for anything. Neither `/verify` nor `/banks` turned up any
> caller in either repo during this same pass, though that's a side
> observation from b-a-i's own greps, not yet Part b-b's own dedicated
> check. **Deliberately no recommendation rendered here** — whether
> these facts make extending `requireInternalApiKey` to `/pay` actually
> appropriate is Part b-a-ii's job, not done this round. Full write-up
> under Task 42's own "Part b-a-i" entry. **Next: Part b-a-ii** (the
> verdict itself), or Part b-b (the same investigation for
> `/verify`/`/banks`), or the still-separately-open webhook-handler/
> verification-API gap the previous note below flagged — three
> genuinely independent open threads, pick whichever the product owner
> prioritizes.
>
> **Newest note (2026-09-02, previous) — the response-parsing "b"
> flagged below is now built, correcting a wrong guess from the
> session that flagged it.** Re-fetched Korapay's own payout docs
> directly rather than trust the prior session's characterization —
> the real shape is two levels, not a flat string: top-level `status`
> genuinely IS a boolean (the existing check was already correct for
> that), and a SEPARATE field, `data.status`, is the string
> (`"processing"` in Kora's own documented example) this code never
> looked at at all. Fixed: `"processing"` is now explicitly treated as
> the normal, expected, non-error outcome (Kora's own docs are clear
> payout confirmation is asynchronous); a new defensive check throws
> on `data.status === 'failed'` (a real, if less common, synchronous
> failure the old code would have silently swallowed as success); logs
> now say explicitly what Kora's transaction status actually is,
> instead of an unqualified "Payout success" for a merely-accepted
> transaction. Verified via `node --check` + 6 functional test cases,
> all passing. **Real, separate gaps surfaced (not fixed) by this
> work, flagged rather than silently left implicit: no webhook handler
> for payout completion anywhere in this repo, and no Payout
> Verification API call either** — without either, this backend has no
> way to ever learn a `"processing"` payout's true final outcome.
> **Next: Part b** (is extending `requireInternalApiKey` to
> `/pay`/`/verify`/`/banks` even appropriate — a design question, not
> an implementation task) **or building the missing webhook handler /
> verification call just flagged** — both genuinely open, pick
> whichever the product owner prioritizes. Full write-up under Task
> 42's own "The 'b' this split implies" section.
>
> **Newest note (2026-09-01, previous) — Task 42 Part A: CRITICAL
> security fix, `POST /payout` had zero authentication, now fixed.**
> Found by a Mavins-web session but flagged in the wrong repo's
> handover (this one has the actual vulnerable code) — confirmed
> directly against this repo's `routes.js` before building anything,
> not taken on trust: any unauthenticated request from anywhere could
> trigger a real Korapay payout to an arbitrary bank account. New
> `requireInternalApiKey` middleware (`utils/helpers.js`), shared
> secret via `X-Internal-Api-Key` header, `crypto.timingSafeEqual`
> comparison, fails closed if the env var itself is unset. Applied to
> `POST /payout` only — verified via `node --check` + 6 functional
> test cases, all passing, including the critical
> env-var-unset-fails-closed case. **`INTERNAL_API_KEY` still needs a
> real value set in Render's dashboard** — nothing is actually
> protected in production until that happens, this is a code fix
> only. **Next: Task 42 Part B** — extend the same protection to
> `/pay`/`/verify`/`/banks` (needs checking whether that's even
> appropriate for those routes first) AND independently verify
> `processPayout`'s amount-unit convention against Korapay's real
> payout docs (never confirmed, unlike the collection side). Full
> write-up in Task 42's own entry.
>
> **Newest note (2026-08-31) — PR #3 opened against upstream, closes
> the gap PR #2's merge/close left open.** Confirmed via `git fetch
> upstream` that PR #2 is merged and closed (`upstream/main` at
> `63f72e2`, "Merge pull request #2 from Zapier-codes/main") — the
> plain-`git push`-auto-joins-an-open-PR mechanic that worked during
> PR #2's window no longer applies. Checked `origin/main` against
> `upstream/main` via `git log upstream/main..origin/main` (not
> assumed): exactly 3 commits ahead —
> `f755a40` (feat: full Korapay payout flow + bank list, the real
> substance, verified with `node --check` before recommending a PR for
> it), plus 2 docs-only commits (`adf47d1`, `2d4a7d6`). This sandbox
> has no GitHub authentication (`gh` unavailable, no token, no
> credential helper — confirmed, not assumed) so the PR itself was
> opened by the person running the command, via `gh pr create --repo
> Phoenix-Boss/B-PAY-backend --base main --head Zapier-codes:main`.
> **Result: https://github.com/Phoenix-Boss/B-PAY-backend/pull/3** —
> confirmed via `gh pr create`'s own success output (an authenticated,
> authoritative source; a follow-up unauthenticated `api.github.com`
> double-check hit a rate limit and wasn't needed). **Same
> queue-until-merged posture as PR #2**: a future session should check
> this PR's live status (merged/open/closed) before assuming anything
> about upstream sync, the same way this session checked PR #2's
> status directly rather than trusting an old assumption.
>
> **Newest note (2026-08-30) — new mandatory rule for every session,
> all three repos: focus on building code now, and split whatever task
> you pick into parts, building only one part per session.** Full rule
> in the new "Build-focus + mandatory task-splitting" section right
> after "Unified hand-off command format" near the top of this file.
> Not applied to anything in THIS repo this session — this session
> only added the rule itself (synced from Mavins-web, where it was
> first written and applied) and did not verify or touch this repo's
> own task queue below; the rest of this box's content is unchanged
> and not re-confirmed as of this note.
>
> **Next task in THIS repo: none currently unblocked in code —
> unchanged.** Task 41 (central Korapay webhook gateway) is **built**
> — `webhookGateway.js`, wired into `routes.js`'s Korapay handler and
> `index.js`'s retry sweep, verified via `node --check` + standalone
> functional/signature smoke tests. **This backend's no-database
> architecture is a confirmed, permanent decision, not an open
> question** — product owner confirmed: every app using this as its
> canonical payment gateway already has its own database, so this
> backend verifies Korapay's signature once and forwards, and each
> app's own edge function owns durable recording into its own DB. The
> gateway's in-memory event store is the correct final shape for that,
> not a stopgap awaiting a real database.
>
> **Correction, this session — the "FULLY DONE" claim below was
> premature.** Step 3 (Korapay's dashboard webhook URL) had been set
> to the **bare domain** (`https://b-pay-backend.onrender.com`), not
> the actual route (`/api/webhooks/korapay`) — this repo's root path
> only has a `GET` handler, so every webhook from Korapay was 404ing
> silently, for every project sharing this one URL slot, the whole
> time this box claimed the chain was "genuinely end-to-end live."
> **Now corrected** — the dashboard is updated to the full path as of
> this session. Recording this prominently because it's exactly the
> kind of thing this box exists to prevent: a "confirmed done" claim
> that wasn't actually verified end-to-end (no one had checked
> `/gateway-stats` or looked for a real recorded event — the
> confirmation was taken on trust, not evidence). **Before trusting
> any "confirmed live" claim in this file again, check
> `/gateway-stats` for a nonzero count, don't just take a prior
> session's word for it.**
>
> **The three-step gateway rollout this box used to track — now
> actually verified to include the correct URL, not just claimed:**
> (1) `MAVW_WEBHOOK_URL`/`MAVW_WEBHOOK_FORWARD_SECRET` set on Render's
> dashboard, (2) Mavins-web's Task 42 swapped `korapay-webhook`'s
> signature verification to this gateway's internal one AND that Edge
> Function has been redeployed, (3) Korapay's own dashboard webhook
> URL re-pointed at this backend's **full webhook path**
> (`/api/webhooks/korapay`, corrected this session — was previously
> just the bare domain). **Still not independently confirmed: an
> actual live event landing in `/gateway-stats`** — the URL is now
> correct, but no one has checked yet whether a real webhook has
> actually come through since the correction. Whoever picks this up
> next should check that before assuming the chain is truly live.
>
> "Korapay only" focus is still active otherwise (see "Current focus"
> section below) — everything else Korapay-eligible in this repo's own
> queue is done. Tasks 17–24 that used to live in this file's queue
> have been **migrated to Mavins-web's own `handover.md` as Tasks
> 28–33** — this repo's copies below are historical only, kept for
> context, not to be worked from directly anymore. This repo's Task 16
> entry above also now carries a companion-change note for Mavins-web's
> Task 30 (forwarding `channels`/`default_channel`) — see that note for
> detail.
>
> **Full cross-repo status, as of this note:**
> - **B-Pay-backend** (this repo) — next: **still no code task.**
>   Genuinely idle until new provider API keys arrive (unblocks the
>   "Current focus: Korapay only" tasks below) or a new task is
>   assigned.
> - **Mavins-web** — next: **Task 33 Part 2 (wallet-crediting)** — now
>   unambiguously the real next task, confirmed by that repo's own top
>   box: the entire gateway chain (Tasks 33 Part 1b → 41 → 42) was
>   built specifically to unblock this, and every prerequisite is now
>   done. Task 40 there gives the exact fee-arithmetic rule to follow
>   (Edge Function computes the 5% deposit deduction and hands the RPC
>   an already-net number; the RPC does no math). Check that repo's own
>   top box directly before starting in case it's moved on since this
>   note was written.
> - **Velune** — next: **see `HANDOVER_CAMPAIGN.md` → "8. Not done /
>   open"** in that repo (`Zapier-codes/Velune`). No numbered task
>   queue there — different convention, established by that repo's own
>   sessions. Current real blocker: no live Supabase credentials wired
>   in — unchanged, still open.
>
> **A session does not need to ask permission before cloning another
> repo or switching context between the three** — if the true next
> task lives elsewhere, just clone it and go. Right now that means:
> **the real next task is in Mavins-web, not here** — a session
> starting in this repo should clone Mavins-web and work Task 33 Part 2
> there, not look for something to do in B-Pay-backend's own queue.
>
> **Every session must update this box before ending** — whatever you
> just finished, update "Next task" here (and the matching box in
> whichever other repo's file needs it) so the next session, in any of
> the three repos, orients in one glance instead of reading a
> 1000+ line file end to end.

---

This file is a task queue for Claude sessions on Anthropic's free tier,
where a session can end at any time without warning. **Tasks are
deliberately small — one file, one concern, one commit.** Never try to
do two tasks in one session, and never leave a task half-finished; if
a task turns out to be bigger than it looked, stop, commit whatever is
cleanly done, note the split in this file, and leave the rest as a new
task for the next session.

The number of sessions here is NOT fixed. Add tasks as you find new
issues. Split a task further if it's still too big once you're in it.
There is no target count to hit — the queue is exactly as long as it
needs to be.

---

## This is a 3-repo project — read this before "How every session works" below

This project spans **three separate GitHub repos**, each worked with
this exact same session-handover pattern, but each with **its own**
`handover.md`, its own task queue, and its own "Patches issued so far"
log — there is no single shared file. What ties the three together is
that any one repo's queue can contain a task whose real subject is a
*different* repo (always named explicitly in the task title, e.g.
"Task 17 — Mavins-web: skip fund-wallet/email step..." inside this
file). See "Sibling repos" below for the current list and each one's
push mechanics — they are NOT all identical (this repo uses a
fork→PR flow; confirm each other repo's own mechanics from its own
`handover.md` rather than assuming they match this one).

**The rule that changes your commands:** a task's code changes,
commits, patches, `git am`, and `git push` always happen in the repo
the task is actually *about* — never in whichever repo you currently
have cloned just because that's where you started reading. Concretely,
if you're working this file's queue and the first unchecked task in
order turns out to be a "Mavins-web: ..." or "Velune: ..." task:

1. **Stop making changes here.** Don't touch this repo's own source
   files for that task.
2. Clone (or `cd` into, if it's already cloned this session) the
   target repo — see its URL in "Sibling repos" below.
3. **Read that repo's own `handover.md` in full before doing anything
   else.** What's written about it here is a pointer/synopsis for
   continuity, not the source of truth — that repo's own file may have
   grown, split the task differently, or already have it done. Follow
   *that* file's own "How every session works" (or equivalent)
   section, since a different repo may have different steps (compare:
   this repo's fork→PR flow vs. Mavins-web's direct-push-no-PR flow,
   confirmed different as of this note).
4. Do the task, commit, format-patch, and verify **inside that repo's
   own working directory** (and its own fresh `/tmp` clone for the
   `git am` verification step) — not this one.
5. **The hand-off you give the human must say explicitly which local
   folder to be in**, since they're very likely still sitting in
   whichever repo's folder they last used — and note that a repo's
   local clone directory name doesn't always match the GitHub repo's
   own casing (confirmed this session: `Mavins-web` on GitHub is
   cloned locally as `mavins-web`, lowercase — check "Sibling repos"
   below for each repo's actual local folder name rather than assuming
   it matches the URL). Template:
   ```
   cd ~/<other-repo-local-folder>    # NOT this repo's folder, and NOT
                                      # necessarily the GitHub-repo casing
   git am ~/storage/downloads/NNNN-....patch
   git push origin main              # confirm against that repo's own
                                      # handover.md — B-Pay-backend and
                                      # Mavins-web both push directly
                                      # (one to a fork+PR, one straight
                                      # to main), don't assume every
                                      # repo does
   ```
6. Update **that repo's own** `handover.md` task queue and "Patches
   issued so far" log — not this file's — as part of the same commit,
   per that repo's own process.
7. If it makes sense for continuity, leave a short one-line pointer
   back in *this* file's own task entry noting what was found/done in
   the other repo (this file already does this in a few places — see
   "Cross-repo continuation" further down) — but the actual work
   record lives in the other repo's own file, not duplicated here.

**A session that's only told "clone repo X" should not assume the
whole queue lives inside repo X.** Read repo X's `handover.md` fully
first; if its first unchecked task is a cross-repo pointer, follow it
into the other repo per the steps above and treat this as one
continuous session working across two repos, rather than stopping and
asking the human to re-invoke you on the other repo. The "Sibling
repos" block immediately below exists specifically so a session that
starts in any one of the three repos can discover the other two exist
and jump on its own.

### Sibling repos (keep this block, or an equivalent, consistent across every repo's own `handover.md`/campaign-doc file — now four repos, not three, see the new `B-PAY` entry added this session)
- **`B-Pay-backend`** (this repo) —
  `https://github.com/Zapier-codes/B-Pay-backend` — fork of
  `https://github.com/Phoenix-Boss/B-PAY-backend`; changes go through
  the fork→PR flow described in "Pull request workflow" below (commit
  → patch → human `git am` + `git push origin main` → auto-joins the
  one open PR against upstream).
- **`Mavins-web`** — `https://github.com/Zapier-codes/Mavins-web` —
  confirmed **not** a fork (checked directly via the GitHub API this
  session: `fork: false`, no parent repo) — no PR step; the human runs
  `git am` **followed by `git push origin main`** directly against
  `main` (a direct push is the whole delivery there, not a step toward
  a PR — see that repo's own `handover.md` for the full write-up).
  **Local clone directory is `mavins-web`, lowercase** — not
  `Mavins-web` — even though the GitHub repo itself is capitalized;
  using the capitalized form for `cd` will fail with "not a git
  repository". (Correction added this session — an earlier pass
  through this file omitted the push step and used the GitHub-repo
  casing for the local folder; both are now fixed here and in
  Mavins-web's own copy of this block.)
- **`Velune`** — `https://github.com/Zapier-codes/Velune` — NOT a
  fork, direct push to `main`. Has its own `HANDOVER_CAMPAIGN.md` (this
  bullet's earlier claim that no handover file existed there yet is
  stale — corrected here, not left standing).
- **`B-PAY`** — `https://github.com/Zapier-codes/B-PAY` — **new this
  session, all future sessions should clone this repo** — fork of
  `https://github.com/Edges-Enterprise/B-PAY` (both confirmed real via
  direct `git ls-remote`, correct casing is all-caps `B-PAY`, not
  lowercase `bpay` as an earlier note in this file guessed before
  either URL had been checked). This is the "Bpay app" — a React
  Native/Expo wallet/banking app, the actual end-user-facing product
  this backend (`B-Pay-backend`) is meant to serve as the sole payment/
  payout source of truth for (see Task 43's own architecture note,
  above/below depending on where this file's grown to by the time you
  read this). **Product owner's own instruction, this session: clone
  it so the bank/payment integration between this backend and that app
  can actually be completed** — not yet begun as of this note; whoever
  picks that up should start by reading this repo's Task 43 in full.
  Same fork→PR mechanics as this repo's own relationship to
  `Phoenix-Boss/B-PAY-backend` — push to `Zapier-codes/B-PAY`, PR to
  `Edges-Enterprise/B-PAY`, not the other direction.

---

## Unified hand-off command format — MANDATORY, every session, all three repos

**This section is the single source of truth for how a session's final
message must be formatted.** It exists because past sessions gave this
in inconsistent shapes (separate blocks per repo, missing push steps,
wrong casing) and the human had to ask for it to be fixed. This
section, or an identical copy of it, must exist in all three repos'
handover files — if you edit it here, copy the same edit into
Mavins-web's `handover.md` and Velune's `HANDOVER_CAMPAIGN.md` in the
same session, the same rule this project already applies to the
"Sibling repos" block.

**The rule:** whenever a session finishes work — in one repo or more
than one — the final message must end with **one single,
copy-pasteable, `&&`-chained command line**, covering every repo
touched this session and nothing else. Never separate command blocks
per repo. Never prose interleaved between repos. Never a bare `git am`
+ `git push` with no `cd`, and never omit the `git push` "because it
was already shown once." One line, chained straight through:

```
cd ~/<repo-1-local-dir> && git am ~/storage/downloads/<repo-1-slug>-<description>.patch && git push origin main && cd ~/<repo-2-local-dir> && git am ~/storage/downloads/<repo-2-slug>-<description>.patch && git push origin main
```

Extend with more `&& cd ~/<repo> && git am ... && git push ...`
segments for however many repos were actually touched. If only one
repo was touched, the chain is just that one repo's three-command
segment — still exactly this shape, not a shorter/different one.

**Filling it in — fixed rules, don't improvise per session:**

1. **Patch filenames are always `<repo-slug>-<short-description>.patch`**
   — all-lowercase, hyphenated. The three fixed slugs for this project:
   - `mavins-web`
   - `b-pay-backend`
   - `velune`

   Use these exact slugs regardless of that repo's actual local folder
   casing (below) or the GitHub repo's own casing — the slug is a
   filename label only, so the human can tell at a glance which patch
   is which when several are sitting in Downloads at once, and so
   every session reuses the same three names instead of inventing new
   ones. `<short-description>` is a few hyphenated words for what the
   patch does (e.g. `handover-nav`, `task-14-geo-currency`) — same
   spirit as this project's existing `NNNN-short-description.patch`
   convention, just without the number prefix, since the repo-slug
   prefix now does that job (unambiguous which-repo-which-patch even
   with several in Downloads at once, without needing sequential
   numbers that could collide across three repos' independent
   sessions).

2. **The `cd` target uses that specific repo's real local folder name
   and casing — confirmed, not guessed, and NOT always the same as the
   slug above or the GitHub repo name:**
   - Mavins-web → `cd ~/mavins-web` (lowercase; the GitHub repo itself
     is `Zapier-codes/Mavins-web`, capitalized — the local clone is
     not)
   - B-Pay-backend → `cd ~/B-PAY-backend` (matches GitHub repo casing
     exactly)
   - Velune → `cd ~/Velune` (matches GitHub repo casing exactly)

   If a fourth repo ever joins this project, confirm its real local
   folder name with the human once (don't assume it matches the GitHub
   name), then add it to this list in all three files' copies of this
   section.

3. **Every repo segment ends with its own `git push origin main`**
   immediately after its own `git am` — never batch every `git am`
   first and push once at the end; if one repo's `git am` fails
   partway through a chain, the `&&` chain stops there and repos
   later in the line correctly never run, which is the whole reason
   each push sits right after its own `git am` rather than all pushes
   at the end.
4. **All three repos currently push the same way** — `git push origin
   main`, confirmed for all three as of this note (B-Pay-backend's
   push still auto-joins its existing open PR #2 against upstream, see
   "Pull request workflow" below — that happens automatically on
   push, no extra command). If any repo's push mechanics ever change
   (e.g. a repo moves off a direct-to-main flow), update this section
   and that repo's own "Sibling repos" entry in the same commit — don't
   let them drift apart.
5. **Nothing goes between the repos in the chain, and nothing goes
   after it.** Prose explaining what changed in each repo belongs
   *before* this command block in the same message, not interleaved
   with it or appended after it.
6. **This format applies even for a single-repo session.** A session
   that only touched one repo still ends with this exact one-line
   `cd && git am && git push` shape (just a shorter chain) — not a
   different, shorter format "because it's only one repo." Consistency
   for the human is the entire point of this section existing.

---

## Build-focus + mandatory task-splitting — MANDATORY, every session, all three repos

**Added to all three repos' handover files this session (2026-08-30),
kept identical the same way the section above it is — if you edit
this section, copy the same edit into the other two in the same
session.**

**Direct product-owner instruction, two parts:**

1. **All sessions should focus on building the code now, fully** — the
   discovery/diagnosis-heavy phase this project spent a lot of recent
   sessions in (schema queries, cross-repo diagnoses, architecture
   proposals) should give way to actually implementing what's already
   been decided. A task that's still genuinely blocked on a real open
   product question stays blocked — don't force an answer that isn't
   there — but a task sitting on a *resolved* decision with nothing
   left but to write the code is exactly what a session should pick
   next, in preference to opening a new discovery thread.
2. **Every session must split whatever task it picks into parts, and
   build only one of those parts** — never the whole task in one go,
   regardless of how small the task looks at a glance. This formalizes,
   as a standing rule rather than an occasional judgment call, the
   pattern this project has already used successfully several times
   (this repo's own Task 33 Part 2's a/b/c/d split; Mavins-web's Task
   46's a/b/c/d/e split, Task 48-b/48-c/48-d's own lettered sub-splits)
   — each part stays independently reviewable, independently
   revertible, and independently patchable, and the natural stopping
   point after one part keeps a single session's diff small enough to
   actually verify properly (`tsc`/`node --check`, targeted checks, a
   throwaway comparison script) rather than ballooning into something
   no one part of which got real scrutiny.
   **Amended (2026-09-01, later still), per explicit product-owner
   instruction: cap the split at 5 parts, lettered a through e.** A
   task doesn't need all 5 — 2 parts (a/b) is completely fine when
   that's the natural shape, same as Mavins-web's Task 59 Part 2b-b's
   own A/B split — but never split into more than 5. If a task's
   natural granularity seems to want a 6th part, that's a signal the
   task itself is too big for one split and should be broken into two
   separate top-level tasks (each with its own up-to-5-part split)
   rather than stretched to 6+ lettered sub-parts under one task.

**How to split, in practice:** before writing any code, write out the
task's natural parts (even if the task text doesn't already list them —
most won't yet, since this is a new standing rule) as their own labeled
sub-entries in the handover file, the same way this repo's own Task 33
Part 2 or Mavins-web's Task 46 entries list their own lettered parts.
Pick the first genuinely unblocked part, build only that one, and leave
the rest explicitly marked not-started for the next session — don't
silently keep going into part two because it "was right there." If a
task turns out to have exactly one indivisible unit of work (rare, but
possible for something truly small), that's fine — say so explicitly
in the write-up ("not split further, this is a single atomic change")
rather than leaving it looking like a part was skipped.

---

## How every session works

1. **Pull latest first.** `git status --short` and `git log --oneline -5`
   to see where the repo actually is. If there are local commits ahead
   of what you expect, that's fine — just don't redo work.
2. **Check whether the previous commit and PR actually landed, before
   doing anything else.** Because of the two-hop fork→PR flow (see
   "Pull request workflow" section below), a commit can be pushed to
   `origin/main` (our fork) and *still* be sitting in an unmerged PR —
   don't assume "it's on origin/main" means "the real owner has it."
   Concretely:
   ```
   git remote add upstream https://github.com/Phoenix-Boss/B-PAY-backend.git 2>/dev/null
   git fetch origin && git fetch upstream
   git log --oneline origin/main -3
   git log --oneline upstream/main -3
   ```
   - If `origin/main`'s latest commit isn't `upstream/main`'s latest
     (or an ancestor of it) — PR #2 is **still open, unmerged**. That's
     the expected, intentional steady state (see "Pull request
     workflow" below — we're deliberately accumulating every session's
     commits into this one PR until Phoenix-Boss merges it all at
     once), so it doesn't block starting a new task. Just confirm PR
     #2 itself is still open and still the one and only PR (don't
     create a second one — see below), and note the current state
     plainly to the human when you hand off this session's patch.
   - If a human-provided update says a PR *was* merged, still verify
     it here against `upstream/main` yourself rather than taking the
     claim at face value — merges can be delayed, rejected, or land on
     a different branch than expected.
   - Either way, update the "Outstanding PRs status" note near the end
     of the "Pull request workflow" section below to reflect what you
     found, so the next session doesn't have to re-derive it.
3. **Read this whole file**, especially the "Confirmed research
   findings" section below — it exists so you don't have to re-derive
   facts a previous session already verified. Then find the **first
   unchecked `[ ]` task** in the queue, in order, and do only that one.
4. **Do the task.** Read the actual current code before changing
   anything — comments in this file describe what was true when
   written, but the previous session's own commit may have already
   changed things.
5. **Verify before committing.** This repo has no test suite yet, so
   at minimum run `node -e "import('./index.js')"` won't work without
   real env vars — instead use `node --check <file>` on every file you
   touched (pure syntax check, no env/network needed), e.g.:
   ```bash
   node --check providers/korapay.js
   node --check routes.js
   ```
   If your task adds a new dependency, run `npm install` and confirm
   it succeeds. If you changed `utils/helpers.js` currency/amount
   logic, write a tiny throwaway `node -e "..."` snippet to sanity
   check the math by hand before committing (delete the snippet after).
6. **Commit.**
   ```bash
   git add -A
   git commit -m "type(scope): short description — Task N"
   ```
   Write a real commit body explaining what you found and why you did
   it this way, the same level of detail as the "Confirmed research
   findings" entries below — the next session (and the human) reads
   this instead of your reasoning trace.
7. **Generate the patch.**
   ```bash
   git format-patch -1 HEAD -o /mnt/user-data/outputs
   mv /mnt/user-data/outputs/0001-*.patch /mnt/user-data/outputs/NNNN-short-description.patch
   ```
   Use the next free 4-digit number (check what's already in
   `/mnt/user-data/outputs` and in this file's "Patches issued so far"
   log below so numbers don't collide across sessions).
8. **Verify the patch actually applies** before handing it over —
   clone the repo fresh to `/tmp`, reset to the commit *before* yours,
   `git am` the patch, confirm it applies cleanly and `node --check`
   still passes. This has caught real mistakes before (see Mavins-web
   project history) — always do it, it takes seconds.
9. **Present the file** with the `present_files` tool so the human can
   see and download it.
10. **Tell the human the exact commands to run, using the "Unified
   hand-off command format" section near the top of this file —
   verbatim, every time.** That section is now the single source of
   truth for this (**no `gh pr create` line ever** — see "Pull request
   workflow" below; PR #2 is already open and reused automatically on
   every push). Do not write a one-off command block that skips that
   section's format, even for a single-repo session.
11. **Check the box** for the task you just did in this file as soon
    as the commit is confirmed **pushed** to `origin/main` (which
    auto-joins PR #2 — see below) — do NOT wait for the owner
    (Phoenix-Boss) to actually merge anything. Pushing the commit is
    this project's definition of "done" for a task; merge timing is
    the real owner's call, on their own schedule, and isn't something
    a session should block on or keep re-checking. Add a short "what
    was found / what changed" note under the task (same style as Task
    3/Task 4 in the Mavins-web project's handover.md — that project is
    the reference example for how this whole process should read),
    and if you know PR #2 is still unmerged as of this session, say
    so plainly in that note (e.g. "pushed, part of PR #2, not yet
    merged by Phoenix-Boss") rather than implying it landed upstream —
    that's what step 2's "Outstanding PRs" check-in is for on the
    *next* session, not a reason to leave this task's box unchecked
    now. Commit that edit to `handover.md` **as part of the same
    commit** as the code change (one commit, one patch, per session —
    don't split the code change and the checkbox update into two).
    split the code change and the checkbox update into two).

---

## Pull request workflow (fork → upstream) — read this before step 10 above

**This repo is a fork.** Confirmed directly against GitHub (not
assumed): `Zapier-codes/B-PAY-backend` is forked from, and its
`network_root_nwo`/parent is, `Phoenix-Boss/B-PAY-backend`. That
second repo is the **real owner's** repo — `Phoenix-Boss` — and only
they can merge into it. Every session's work reaches the real
codebase in two hops: (1) push to our own fork's `main`, (2) that
code sits in a pull request until Phoenix-Boss merges it.

**⚠️ There is already ONE open PR that covers this whole project —
PR #2 (`https://github.com/Phoenix-Boss/B-PAY-backend/pull/2`),
`Zapier-codes:main` → `Phoenix-Boss:main`. No session should ever run
`gh pr create` (or the browser compare-URL) again for this repo.**
GitHub only allows one open PR per branch pair, and — confirmed
directly by trying it — a second `gh pr create` attempt just fails
with `a pull request for branch "Zapier-codes:main" into branch
"main" already exists`, pointing back at PR #2. This isn't a fallback
behavior to guard against, it's the whole point: **every future
session's commit, once pushed to `origin/main`, joins PR #2
automatically** — GitHub appends new commits on a branch to whatever
open PR already exists for that branch, with zero extra command
needed. So step 10 above is now just `git am` + `git push origin
main`, full stop.

**Why we're doing it this way (explicit project decision, not a
guess):** the plan is to leave PR #2 open and keep accumulating every
session's commits into it — task after task — **until all the fixes
in this project's task queue are actually done**, rather than opening
and closing a separate small PR per task. Phoenix-Boss then reviews
and merges the whole batch **once, in one shot**, at whatever time is
convenient for him. This is intentional, not a workaround — don't
"helpfully" split things into smaller PRs, don't close PR #2 early,
and don't ask the human to merge anything on the fork side to try to
"clean up" — the fork's `main` accumulating commits *is* the plan.

**What this means for step 11 (marking a task done):** unchanged in
spirit from before — check the task's box as soon as its commit is
pushed to `origin/main`, don't wait for Phoenix-Boss to merge. The
only difference is there's no PR-open confirmation step anymore since
there's nothing new to open — pushing is now the entire finish line.

**What a session should still verify at the top of every session
(step 2 above):** that PR #2 is still open (hasn't been merged or
closed out from under this plan) and still targeting the right branch
pair. If a session's step-2 check ever finds PR #2 has been merged —
i.e. `upstream/main`'s latest commit is no longer `900db65` — that's
a real state change worth flagging clearly to the human (see the
status line below), since it may mean the queue-until-done plan needs
revisiting or a fresh PR will eventually be needed for whatever's
still unmerged. Until that happens, "PR #2 open, accumulating
commits, unmerged" is the fully expected steady state — not something
to chase, escalate, or try to fix.

**Outstanding PRs status (updated by whichever session last checked —
see step 2 above):** **PR #2 has been merged by Phoenix-Boss**,
confirmed this session by adding the real `upstream` remote
(`https://github.com/Phoenix-Boss/B-PAY-backend.git`) and fetching it
directly (not assumed, not inferred from the fork alone) —
`upstream/main`'s latest commit is now `63f72e2`, "Merge pull request
#2 from Zapier-codes/main", which brings in everything through
`01df9c7` (this fork's own latest at the time of checking). This is
the real state change the note above this one anticipated — **the
queue-until-done plan has now reached its natural endpoint for
everything committed so far.** Nothing in this fork's `main` is
ahead of what's now live upstream. Per that same note: whenever the
next task's work is ready to ship, a **fresh PR** will need to be
opened (PR #2 is closed/merged, it won't silently keep absorbing new
commits the way it did while open) — don't assume `git push
origin main` alone is still sufficient the way it was during PR #2's
window; check whether a new PR needs creating before assuming a plain
push is the whole story next time.

---

## Confirmed research findings (verified against primary sources — don't re-derive these, but do re-verify the specific endpoint page before shipping a task that depends on one)

**Paystack — FULL API discovery pass, audited 2026-09-06 (supersedes
all prior Paystack entries below; nothing from the prior audit was
dropped, only expanded/corrected).** Every source cited below was
fetched directly this session from paystack.com/docs — the API
Reference (Introduction, Authentication, Rate Limits, Pagination,
Errors) and the Transactions endpoint page, plus a re-fetch of the
Webhooks guide to re-confirm what the 2026-08-27 pass already found.
This covers the Transactions API **in full** (every endpoint Paystack
documents under that resource, not just the two this repo currently
calls), the six-currency support table, the error-response envelope,
and documented rate limits, on top of re-confirming the webhook
signature scheme. It does **not** cover Transaction Splits, Terminal,
Virtual Terminal, Customers, Direct Debit, Dedicated Virtual
Accounts, Preauthorization, Apple Pay, Capitec Pay, Subaccounts,
Plans, Subscriptions, Products, Storefronts, Orders, Payment Pages,
Payment Requests, Settlements, Transfer Recipients, Transfers,
Transfers Control, Bulk Charges, Integration (session-timeout),
Charge (the PIN/OTP/phone/birthday/address step-up flow), Disputes,
Refunds, or Verification (Resolve Account Number/Validate Account/
Resolve Card BIN) — those are real, separate Paystack API resource
families with their own doc sections, listed here as a known,
explicit exclusion rather than an accidental gap (none of them are
referenced anywhere in this repo's code today — if a future task
needs Transfers for payouts, or Verification's "Resolve Account
Number" for bank-detail confirmation, it needs its own real discovery
pass, not an assumption borrowed from this one).

*Environment & authentication*
- Base URL: `https://api.paystack.co` — **confirmed exact match**
  with this repo's `getProviderBaseUrl('paystack')` in
  `utils/helpers.js` (identical for both `development` and
  `production` entries there — correct: like Korapay, Paystack has no
  separate sandbox host; test vs. live is determined entirely by
  which key-pair is used against this one host). Source:
  paystack.com/docs/api/.
- Auth: `Authorization: Bearer {SECRET_KEY}` on every request —
  confirmed at paystack.com/docs/api/authentication/ ("Every request
  must include your secret key in the Authorization header, using the
  Bearer scheme") — matches this repo's existing
  `Bearer ${this.secretKey}` usage in both methods in
  `providers/paystack.js`. Minor, harmless observation: the verify
  call is a bodyless `GET` but this repo still sends a
  `Content-Type: application/json` header on it — dead weight, not a
  correctness issue (Paystack ignores it on a bodyless GET).

*Initialize Transaction — `POST /transaction/initialize`*
- **Path confirmed exact match** against
  paystack.com/docs/api/transaction/#initialize — `processPayment`'s
  call site already uses this path correctly. Task 8's premise that
  this endpoint "was already believed correct going in" is now a
  **confirmed correct**, not just a belief.
- Required body params confirmed: `email` (string), `amount` (subunit
  of the currency — see currency table below). This repo sends both.
- `currency` is documented as **optional**, defaulting to the
  integration's own configured currency if omitted — this repo always
  sends it explicitly (`data.currency`), which is fine (arguably
  safer — removes ambiguity about which currency a request lands in)
  but is a deliberate choice worth knowing about, not a requirement.
- Optional params confirmed to exist that this repo does **not**
  currently send (documented, unused capability, not bugs — noted so
  a future task doesn't have to re-discover them): `channels` (array
  — restrict which payment channels Checkout offers: card, bank,
  apple_pay, ussd, qr, mobile_money, bank_transfer, eft, capitec_pay,
  payattitude), `callback_url` (per-transaction redirect override),
  `metadata` (stringified JSON — no metadata pass-through exists in
  this repo today), and `split_code`/`subaccount`/
  `transaction_charge`/`bearer` (marketplace-style payment splitting
  — not relevant unless/until this platform needs to split a payment
  across multiple destination accounts).
- Response shape **confirmed exact match** against the docs' own
  sample: `{ status: true, message: "Authorization URL created",
  data: { authorization_url, access_code, reference } }` — this
  repo's `responseData.status` check and its reliance on
  `data.authorization_url`/`data.reference` downstream both line up
  with what Paystack actually returns.
- `reference`: optional at the API level (Paystack generates one if
  omitted), but this repo always generates/passes its own via
  `generateReference('paystack')`, consistent with Task 12's
  idempotency work. Character restriction **confirmed exact match**:
  docs state "Only `-`, `.`, `=` and alphanumeric characters allowed"
  — `routes.js`'s regex (`/^[A-Za-z0-9\-.=]+$/`) enforces exactly
  that set, no more and no less.

*Verify Transaction — `GET /transaction/verify/:reference`*
- **Path confirmed exact match** against
  paystack.com/docs/api/transaction/#verify — same "already believed
  correct, now confirmed" status as Initialize above.
- **Real finding, not just a docs confirmation — queued as Task 8c
  rather than fixed here, since this pass is documentation-only per
  Task 8's own scope:** Paystack's Errors page states explicitly, in
  its own HTTP-codes table: "Note that we will always send a 200 if a
  charge or verify request was made. Do check the data object to know
  how the charge went (i.e. successful or failed)." The top-level
  `status: true` / `message: "Verification successful"` Paystack
  returns on a verify call describes **the API call succeeding**, not
  **the transaction succeeding** — a verify call for a genuinely
  failed or abandoned transaction still comes back HTTP 200 with
  top-level `status: true`; only the nested `data.status` field
  (`"success"` / `"failed"` / `"abandoned"`) says what actually
  happened to the money.
  `providers/paystack.js#verifyTransaction` only throws on
  `!response.ok || !responseData.status` — it never inspects
  `responseData.data.status` — so it returns normally (no throw) for
  a failed or abandoned transaction. `routes.js`'s `GET /api/verify`
  route then reports that back to its caller as `{ status: true,
  message: 'Verification successful', ... }` unconditionally, with
  the real per-transaction outcome buried in `data.data.status` and
  never surfaced or checked. This is exactly the "paid but no value"
  class of bug Paystack's own webhooks guide warns integrators about
  — a caller trusting the outer `status`/`message` fields (the
  natural reading of this route's own response shape) would treat a
  declined card or an abandoned checkout as a confirmed payment.
- Response shape for a *successful* verification is otherwise
  **confirmed exact match**: `id`, `status`, `reference`, `amount`
  (subunit), `currency`, `channel`, `paid_at`, `customer{...}`,
  `authorization{...}`, `fees`, `gateway_response`, `log{...}`, etc.
  This repo doesn't destructure any of these fields itself (the whole
  `responseData` is passed back up through `routes.js` as-is), so
  there's no field-name mismatch to find — the only real issue is the
  status-check gap above, not the shape.

*Supported currencies & amount units*
- **Real finding: the confirmed-currency list is incomplete.**
  Paystack's own API Reference ("Supported currency" section) lists
  **six** currencies, not five:

  | Currency | Subunit | Min. transaction | Availability |
  |---|---|---|---|
  | NGN | Kobo | ₦50.00 | Nigeria |
  | USD | Cent | $2.00 | Kenya and Nigeria |
  | GHS | Pesewa | ₵0.10 | Ghana |
  | ZAR | Cent | R1.00 | South Africa |
  | KES | Cent | Ksh.3.00 | Kenya |
  | **XOF** | *(none — see below)* | XOF 1.00 | Côte d'Ivoire |

  `utils/helpers.js`'s `CONFIRMED_PROVIDER_CURRENCIES.paystack` lists
  only `['NGN', 'GHS', 'ZAR', 'KES', 'USD']` — **XOF is missing.** Not
  a mistake at the time the list was written (the prior session's
  cited sources — Chargebee, Zoho, mctaba.com — are third-party
  integration guides describing only Paystack's five better-known
  currencies; none mention XOF), but Paystack's own primary docs now
  list a sixth, real, live-supported currency for Côte d'Ivoire
  integrations. Doesn't block anything today if this platform has no
  Côte d'Ivoire-facing integration yet, but the list can't accurately
  call itself "confirmed against primary sources" while missing an
  entry the primary source itself lists. Queued as **Task 8d**, not
  fixed here — editing `CONFIRMED_PROVIDER_CURRENCIES` is Task 9/9b's
  territory, not Task 8's.
- XOF has an unusual rule worth flagging even though implementing it
  is out of this pass's scope: "While there is no subunit for XOF,
  developers must multiply the amount by 100 regardless." — the ×100
  multiplier this repo already applies uniformly
  (`getAmountFormat`'s `{ unit: 'subunit', multiplier: 100 }` for
  every Paystack currency) would, unusually, still be *correct* for
  XOF too, even though XOF has no real subunit. So Task 8d, when
  picked up, is a pure addition to the array, not a new branch or a
  multiplier change.
- For the five currencies already in the list, ×100 subunit
  conversion and the `NGN`/`GHS`/`ZAR`/`KES`/`USD` set itself are
  **both re-confirmed, exact match** — no changes needed there.
- Per-currency **availability is account-level, not something this
  code can or should try to validate itself**: e.g. USD only works
  "for businesses in Nigeria and Kenya" per Paystack's own docs — a
  single Paystack account can't accept all six currencies regardless
  of what this repo sends. Noted here as a real operational
  constraint (a request in a currency the account isn't configured
  for will fail on Paystack's side with a validation error, not on
  this repo's side), not as a code bug — there's no control-flow
  change this repo could make to fix it.

*Error response format — newly documented here, not previously in
this file*
- Confirmed at paystack.com/docs/api/errors/: every error response
  shares the envelope `{ status: false, message, meta: { nextStep,
  ... }, type: "api_error" | "validation_error" | "processor_error",
  code }`. HTTP codes: 200 (success **or** a charge/verify call that
  completed but describes a failure in `data` — see the Verify
  finding above), 201, 400 (validation/client error), 401 (bad or
  missing secret key), 404, 5xx (Paystack-side). This repo's
  `throw providerError(responseData.message || '...')` in both
  `processPayment` and `verifyTransaction` only ever surfaces
  `message` — `type` and `code` (e.g. `missing_params`, or
  processor-specific codes) are discarded. Not a bug — `message` is
  explicitly documented as "the only key that's universal across
  requests," and Task 13 already marked Paystack's `message` safe to
  surface to end users — but `type`/`code` would be useful for
  programmatic branching (e.g. treating `processor_error` differently
  from `validation_error`) if a future task ever needs that
  distinction. Not queued as its own task; noted as available-but-
  unused, same treatment as the unused `channels`/`metadata`/split
  params above.

*Rate limits — newly documented here, not previously in this file*
- Confirmed at paystack.com/docs/api/rate-limits/: standard limit is
  **600 requests/60s live, 100 requests/60s test**, per integration,
  applied per-endpoint (not a single combined budget across the whole
  integration). `/transaction/verify` specifically has an **extended**
  limit of 3,000 requests/60s (live only — test mode always uses the
  standard limit); `/transaction/initialize` gets 1,200/60s. A `429`
  comes back with the same error envelope as above
  (`code: "rate_limited"`) plus `x-ratelimit-reset`/`-remaining`/
  `-limit` headers. This repo has no retry/backoff handling for `429`
  today in either `processPayment` or `verifyTransaction` — not
  flagged as a bug at current traffic levels (there's no evidence
  this repo is anywhere near these limits), but worth knowing exists
  if traffic grows, especially since Paystack's own docs specifically
  call out "polling `/transaction/verify/:reference` in a loop" as
  the most common cause of hitting it — this repo's `/api/verify`
  route is called on-demand per-request, not polled in a loop, so
  it's not currently doing the thing Paystack warns about, but a
  future retry/polling feature should keep this limit in mind.

*Webhook signature — re-confirmed this session, no corrections needed*
- Header `x-paystack-signature`, value is a hex-encoded **HMAC-SHA512**
  of the event payload, keyed with the secret key — re-fetched and
  re-confirmed against paystack.com/docs/payments/webhooks/ plus
  Paystack's own public Node.js webhook example (identical
  `crypto.createHmac('sha512', secret).update(JSON.stringify(req.body))`
  pattern). `providers/paystack.js#verifyWebhookSignature` still
  matches exactly — no changes needed. Nuance already recorded from
  the original 2026-08-27 pass, unchanged: Paystack's own official
  Node example computes the hash over `JSON.stringify(req.body)` —
  the body **after** `express.json()` has parsed and re-serialized it
  — not the raw request bytes. Task 2's speculative note (that
  Paystack needs true raw bytes) turned out to be an overcautious
  guess for this specific provider; implemented to match the primary
  source exactly, with a code comment flagging the re-serialization
  fragility this implies. No `express.json({ verify })` change was
  needed for Paystack.
  Also re-confirmed: there is **no dedicated "charge failed" event**
  in Paystack's supported-events list — `charge.success` is the only
  charge-related webhook; failures simply don't raise one. Full event
  list (for future reference): charge.dispute.create/remind/resolve,
  charge.success, customeridentification.failed/success,
  dedicatedaccount.assign.failed/success, invoice.create/
  payment_failed/update, paymentrequest.pending/success, refund.
  failed/pending/processed/processing, subscription.create/disable/
  expiring_cards/not_renew, transfer.failed/success/reversed.
  This repo implements `POST /api/webhooks/paystack` (Task 3):
  verifies the signature (401 on failure/missing), logs
  `charge.success` transactions, and logs-only for every other event
  type (no persistence layer exists yet — see Task 12).

**DodoPayments — FULL API discovery pass, audited 2026-09-06 (new —
this is a-4's first-ever discovery pass; no prior DodoPayments entry
existed to supersede).** This is a **documentation-only pass** per
Task 0's Discovery Convention: no code was written, `providers/`
has no `dodopayments.js` yet, and none of this repo's route/helper
files reference DodoPayments in any way. Every source below was
fetched directly this session from docs.dodopayments.com (API
Reference: Introduction/Authentication/Rate Limits/Error Codes,
the Checkout Sessions and Payments endpoint pages, the Webhooks
guide, the Adaptive Currency / per-currency pages) plus the
`dodopayments/dodo-docs` and `dodopayments/dodopayments-node`
GitHub repos for SDK-level confirmation. This is a **Merchant of
Record** platform (Dodo becomes the seller of record, unlike
Korapay/Paystack/Juicyway/Payscribe which are payment processors
only) — flagged up front because it changes the compliance shape of
routing through it, not just the API shape: tax collection/remittance
and being the legal seller are Dodo's job, not this platform's, for
any transaction routed there. This audit covers Authentication,
Checkout Sessions (the primary/recommended integration surface),
the legacy Payments API, currencies/settlement, the error envelope,
rate limits, and the webhook signature scheme. It does **not** cover
Subscriptions, Products, Customers, Discounts, Refunds, Disputes,
License Keys, Usage-Based Billing/Meters, Credit-Based Billing,
Balance Ledger, Brands, Payouts, or the Customer Portal — those are
real, separate DodoPayments resource families with their own doc
sections, listed here as a known, explicit exclusion (this platform's
current no-DB, payin/payout-routing-only scope per Task 0 doesn't
need most of them yet; if a future task needs Refunds or Disputes
for payout reconciliation, that needs its own real discovery pass,
not an assumption borrowed from this one).

*Environment & authentication*
- Base URLs: **`https://test.dodopayments.com`** (test mode) and
  **`https://live.dodopayments.com`** (live mode) — confirmed at
  docs.dodopayments.com/api-reference/introduction. Unlike
  Paystack/Korapay (one host, key-pair determines mode),
  DodoPayments uses **two separate hosts** — this repo's existing
  `getProviderBaseUrl(provider)` pattern in `utils/helpers.js` (one
  `development`/`production` URL pair per provider) already
  supports this shape directly: `development: 'https://test.dodopayments.com'`,
  `production: 'https://live.dodopayments.com'` — no structural change
  needed to that function, just a new entry once implementation
  starts. The official Node SDK also takes an explicit
  `environment: 'test_mode' | 'live_mode'` constructor option
  (defaults to `live_mode`) as a second way to select the host —
  this repo would use the base-URL-by-`NODE_ENV` approach already
  established for other providers instead, for consistency.
- Auth: `Authorization: Bearer {API_KEY}` on every request — same
  Bearer scheme as Paystack/Korapay/Juicyway. Two key modes exist at
  generation time (Dashboard → Developer → API Keys): a **write**
  key (full read/write) and a **read-only** key (fetch-only, cannot
  create/modify) — worth knowing for least-privilege key issuance
  once this goes to the product owner for real credentials, but not
  a code-shape difference (this repo already stores one secret key
  per provider).

*Checkout Sessions — `POST /checkout-sessions` (recommended
integration surface)*
- **This is the endpoint to build against, not the legacy Payments
  API below.** Confirmed at
  docs.dodopayments.com/api-reference/checkout-sessions/create: the
  older `POST /payments` endpoint is explicitly marked
  **"Deprecated API"** in its own doc page, with DodoPayments'
  own docs recommending Checkout Sessions instead for both
  one-time payments and subscriptions.
- Required body: `product_cart` (array of `{ product_id, quantity }`
  — `product_id` comes from a product already created in the
  DodoPayments dashboard, not something this repo generates itself,
  a real structural difference from Paystack/Korapay where the
  amount is passed directly in the initialize call). This is a
  **product-catalog-driven** model, not an arbitrary-amount-per-call
  model — relevant to Task 0's business-model note about this
  platform's own per-transaction pricing sitting on top of whatever
  the provider charges: a DodoPayments integration would need
  products pre-provisioned in the DodoPayments dashboard (or
  provisioned via its Products API, not yet audited) rather than
  just passing an amount through, unlike every other provider this
  repo talks to today.
- Common optional fields confirmed to exist (not required, but real
  and documented): `customer` (`{ email, name }` or `{ customer_id }`
  for a returning customer), `return_url`, `billing_address`,
  `billing_currency`, `discount_code`, `metadata`, `confirm` (process
  immediately using a saved `payment_method_id`, skipping the hosted
  page — session validity drops from 24h to 15 minutes when
  `confirm: true` is used), `subscription_data` (trial periods,
  on-demand/mandate-only flows), `allowed_payment_method_types`,
  `force_3ds`, `show_saved_payment_methods`, `feature_flags`
  (e.g. `redirect_immediately` to skip the success page).
- Response shape **confirmed**: `{ session_id, checkout_url,
  client_secret, payment_id, publishable_key }` — `client_secret`
  and `publishable_key` are only populated when `confirm: true`
  created a PaymentIntent at session-creation time, `null`
  otherwise. A companion `GET /checkout-sessions/{id}` (status
  retrieval) and a `POST /checkout-sessions/preview` (calculate
  pricing/tax/totals without creating a real session) both exist,
  documented but not detailed further here since neither is core to
  a first integration pass.
- After payment, the customer is redirected to `return_url` with
  query params including the payment/subscription ID, status,
  customer email, and (for license-key products) the license key
  itself — this repo would need its own `return_url` handler if it
  goes this route, same general shape as Paystack's redirect-based
  `authorization_url` flow.

*Legacy Payments API — `POST /payments` (deprecated, documented here
only because it's the closer conceptual analog to Paystack's
`/transaction/initialize` this repo already knows)*
- Confirmed **"Deprecated API"** per its own OpenAPI-sourced doc
  page (docs.dodopayments.com/api-reference/payments/post-payments) —
  DodoPayments' own docs say "We recommend using Checkout Sessions
  instead." Not recommended as this repo's integration target; noted
  for completeness only, not as a real candidate for `providers/dodopayments.js`.
- `GET /payments/{payment_id}` (**Get Payment Detail** — retrieve a
  specific payment, the closest analog to Paystack's
  `verifyTransaction`) and `GET /payments` (**List Payments**,
  paginated) both confirmed to exist and are **not** marked
  deprecated — these remain the way to check a payment's status
  after the fact regardless of whether Checkout Sessions or the
  legacy endpoint created it.

*Currencies & settlement — real, unresolved discrepancy between two
DodoPayments primary sources, flagged rather than guessed at*
- One primary source (the Error Codes reference,
  `UNSUPPORTED_CURRENCY` entry) states: **"currently supported
  products are only USD and INR"** for product/addon base pricing,
  and that `billing_currency` may only be `USD` or `INR`.
- A different primary source (DodoPayments' own per-currency
  marketing/docs pages, e.g. `dodopayments.com/currency/eur`,
  `/currency/usd`, `/currency/inr`) describes **EUR, USD, INR, GBP**
  (and others with dedicated pages) as **"native settlement
  currencies"** — meaning a business can apparently price and settle
  directly in more than just USD/INR with **zero** Adaptive Currency
  conversion fee, distinct from the 80+ currencies Adaptive Currency
  can *display and convert* at checkout (2–4% fee, borne by customer
  by default) for a business whose base/settlement currency is one
  of the native ones.
- **This repo does not resolve this discrepancy** — it's flagged as
  a real open question for whichever session first writes
  `providers/dodopayments.js`: confirm current product-creation
  currency options directly against a live (even test-mode) Dashboard
  or a fresh API call before assuming either source is stale or
  wrong. Given this platform's own `CONFIRMED_PROVIDER_CURRENCIES`
  convention exists specifically to avoid guessing at a provider's
  currency support (see the Paystack XOF finding above for what
  guessing from secondary sources costs), this is exactly the kind
  of ambiguity that convention exists to catch — no entry has been
  added to `CONFIRMED_PROVIDER_CURRENCIES.dodopayments` this pass,
  and none should be added until this is resolved with a primary
  source that isn't self-contradictory.
- Minimum transaction amounts are **currency-specific, not a single
  base-unit rule** (unlike Paystack/Korapay's flat ×100/×1 rules):
  $0.50 USD, €0.50 EUR, ₹5 INR, 1000.00 CLP, 100.00 BDT, etc., each
  documented on that currency's own page — a future
  `getAmountFormat('dodopayments', ...)` branch would need a
  per-currency minimum table, not just a unit/multiplier pair.

*Error response format — newly documented here*
- Confirmed at docs.dodopayments.com/api-reference/introduction and
  the (non-English-only-available at fetch time, but content
  language-independent) Error Codes reference: every error response
  is `{ code, message }` — e.g. `{ "code": "UNSUPPORTED_COUNTRY",
  "message": "Country AI currently not supported" }`. No `type` field
  and no nested `meta` object the way Paystack has — flatter than
  Paystack's envelope. Standard HTTP status codes used: 400, 401,
  403, 404, 409, 410, 413, 422, 429, 500. `code` is a **stable,
  machine-readable string** (e.g. `CHECKOUT_SESSION_CONSUMED`,
  `PRODUCT_CART_EMTPY` — sic, DodoPayments' own docs note this is a
  deliberate/kept typo matching the API's actual value, not a docs
  error), grouped by API area (Payments & Checkout, Refunds,
  Subscriptions, Products/Brands, Discounts, License Keys,
  Usage-Based Billing, Credit-Based Billing, Wallet, Currency/Tax/
  Region, Validation, General/System) — a real, useful surface for
  programmatic branching that this repo's existing
  `throw providerError(...)` pattern (which only ever surfaces a
  message string for Paystack/Korapay) would need to decide whether
  to preserve or flatten, same open question Task 8's Paystack audit
  raised for `type`/`code` there.
- Distinct from **card-decline reasons** (a separate "Transaction
  Failures" doc page, not audited this pass — DodoPayments' own docs
  explicitly separate "API and business logic errors" from
  "card decline reasons like `INSUFFICIENT_FUNDS` or `CARD_DECLINED`
  returned when a payment fails").

*Rate limits — newly documented here*
- Confirmed at docs.dodopayments.com/api-reference/introduction: a
  **dual-window** system (burst + sustained), tiered by business
  account, **not a single flat number** the way Paystack documents
  per-endpoint limits:

  | Tier | Burst (per second) | Sustained (per minute) |
  |---|---|---|
  | Unauthenticated (by IP) | 20 | 100 |
  | Tier 0 (default) | 40 | 240 |
  | Tier 1 | 100 | 1,000 |
  | Tier 2 | 500 | 5,000 |

  `429` responses use the same `{ code: "TOO_MANY_REQUESTS", message }`
  envelope as any other error, plus `X-RateLimit-Limit`/
  `-Remaining`/`-Reset` headers. Official SDKs (Node/Go/PHP/C#/Java)
  all auto-retry `429`, `408`, `409`, and `>=500` twice by default
  with exponential backoff — this repo has no retry/backoff handling
  for any provider today (same gap already noted for Paystack), so
  this isn't a DodoPayments-specific deficiency, just consistent with
  the existing pattern.

*Webhook signature — newly documented here*
- DodoPayments follows the **Standard Webhooks specification**
  (the same open spec Svix co-created) — a **materially different
  scheme from every provider this repo already integrates**:
  Paystack and Korapay both use a single HMAC hex-digest header
  (`x-paystack-signature`, Korapay's equivalent); DodoPayments
  instead sends **three headers** — `webhook-id`, `webhook-timestamp`,
  `webhook-signature` — and the signed content is the
  **concatenation** `${webhook-id}.${webhook-timestamp}.${raw-body}`
  joined with literal `.` characters, HMAC-SHA256'd with the webhook
  secret (issued in `whsec_...` format, a distinct secret from the
  API key) and **base64-encoded** (not hex), then prefixed with a
  version tag (e.g. `v1,`) — confirmed against
  docs.dodopayments.com/developer-resources/webhooks and
  DodoPayments' own official Express.js example using the
  `standardwebhooks` npm package's `Webhook.verify()` helper.
- **Real implementation implication, flagged for whichever session
  builds this:** this scheme is timestamp-aware by design (Standard
  Webhooks recommends rejecting anything outside a 300-second
  tolerance window to block replay attacks) — a real security
  property Paystack/Korapay's plain HMAC schemes in this repo don't
  have today. A `providers/dodopayments.js#verifyWebhookSignature`
  should either use the `standardwebhooks` package directly (as
  DodoPayments' own docs recommend) rather than hand-rolling the
  HMAC/base64/timestamp logic, or hand-roll it carefully with the
  timestamp check included — not a copy-paste of Paystack's
  `crypto.createHmac('sha512', ...)` pattern, which would be wrong
  on the hash algorithm (SHA256 not SHA512), the encoding (base64
  not hex), the signed content (id+timestamp+body, not just body),
  and the header names, all four at once.
- Full webhook event-type list (for future reference, not all
  relevant to this platform's current no-DB payin/payout scope):
  `payment.succeeded`, `payment.failed`, `payment.processing`,
  `payment.cancelled`, `refund.succeeded`, `refund.failed`,
  `dispute.opened`, `dispute.expired`, `dispute.accepted`,
  `dispute.cancelled`, `dispute.challenged`, `dispute.won`,
  `dispute.lost`, `subscription.active`, `subscription.renewed`,
  `subscription.on_hold`, `subscription.cancelled`,
  `subscription.failed`, `subscription.expired`,
  `subscription.plan_changed`, `subscription.updated`,
  `license_key.created`. Idempotency: DodoPayments' own docs
  recommend using the `webhook-id` header as an idempotency key —
  directly relevant to Task 0's no-DB constraint, since deduplicating
  by that header would need to happen without a database if this
  platform ever needs to guard against duplicate webhook delivery
  for DodoPayments specifically.

**Korapay — FULL API discovery pass, re-audited 2026-09-06 (supersedes
all prior Korapay entries below; nothing from the prior audit was
dropped, only expanded/corrected).** Every source cited was fetched
directly this session from developers.korapay.com (via its own
`llms.txt` index plus the live site's sidebar, which turned out to
list more pages than `llms.txt` does — noted explicitly below rather
than silently using the shorter list). This pass covers every
pay-in, payout, refund, chargeback, balance, and conversion endpoint
Korapay documents; it does **not** cover Card Issuing, Direct Debit,
Identity/KYC, Payment Links, Pool Accounts, Voucher Payments,
Settlements/Audit Logs, or the WooCommerce plugin — those are real,
separate Korapay product lines with their own doc sections, listed
here so they're a known, explicit exclusion rather than an
accidental gap (none of them are referenced anywhere in this repo's
code or in Task 0's own scope — if a future task needs one, it needs
its own real discovery pass, not an assumption borrowed from this
one).

*Environment & authentication*
- Base URL: `https://api.korapay.com/merchant` — **confirmed exact
  match** with this repo's `getProviderBaseUrl('korapay')` in
  `utils/helpers.js` (identical for both `development` and
  `production` entries there, which is correct: Korapay has no
  separate sandbox host — test vs. live is determined entirely by
  which of your two key-pairs you use against this one host, per
  developers.korapay.com/docs/test-live-modes, "No real charge is
  made on payments in Test mode... switching modes does not stop
  active payments in Live mode from going through").
- Auth: `Authorization: Bearer {SECRET_KEY}` on every endpoint seen
  this session (developers.korapay.com/docs/checkout-redirect states
  this explicitly for charges; every other endpoint family's
  documented examples use the same header) — matches this repo's
  existing `Bearer ${this.secretKey}` usage everywhere in
  `providers/korapay.js`. One inconsistency in Korapay's *own* docs,
  flagged as-is rather than resolved: `docs/payout-utilities`'s
  "Get Supported Bank Countries by Currency Code" section says
  "Requests without a valid **public** key will return a `401`" —
  every other endpoint on every other page says **secret** key. Not
  re-derived or guessed at; if a future session calls that specific
  endpoint and gets a real 401 with the secret key, try the public
  key next and update this note with the real answer.
- Public vs. secret keys, test vs. live: developers.korapay.com/docs/api-keys
  — public keys are safe client-side, initiate transactions only;
  secret keys read/write everything and must never leave the server.
  Separate key-pairs per mode, obtained from the dashboard's API
  Configuration tab.

*Pay-ins / Collections*
- `POST /api/v1/charges/initialize` (Checkout Redirect/Standard) —
  **re-confirmed**, no change from the prior audit: `amount` is base
  currency units, not subunits (see prior reasoning, still valid).
  Full parameter table now captured directly from
  developers.korapay.com/docs/checkout-redirect, including three
  fields this repo's `processPayment()` never sends today: `notification_url`
  (per-transaction webhook override — intentionally unused here,
  since Task 41's architecture relies on exactly one dashboard-level
  webhook URL for fanout; sending a per-transaction override would
  bypass that gateway entirely, so this omission is correct, not a
  gap), `metadata` (up to 5 keys, ≤20 chars each, `A-Z a-z 0-9 -`
  only — genuinely unused, would be useful for passing the caller's
  own order id through to the webhook) and `merchant_bears_cost`
  (boolean, defaults to `true` — genuinely unused; whoever eats the
  provider fee is currently whatever Korapay's own account-level
  default is, not something this repo controls per-transaction).
  Response is `{ status, message, data: { reference, checkout_url } }`
  — matches what `processPayment()` already expects.
- `GET /api/v1/charges/:reference` (verify/query a charge) —
  **re-confirmed** working path, matches `verifyTransaction()`
  exactly. Response shape (from the Virtual Bank Account doc's own
  "Charge Query API" example, a fuller example than the checkout page
  gives): `{ reference, status, amount, amount_paid, fee, currency,
  description, customer: {name, email}, virtual_bank_account?:
  {...} }` — note `amount_paid` as a **separate field from `amount`**,
  relevant for partial/underpaid bank-transfer scenarios (see
  "Underpayments" below) — this repo's code doesn't currently read or
  surface `amount_paid` anywhere.
- **New nuance not in the prior audit:** the Checkout Redirect page's
  own webhook example includes `payment_reference` as a field
  alongside `reference`, both equal, but annotates `payment_reference`
  as `// DEPRECATED`. This repo's webhook handler doesn't reference
  `payment_reference` at all today, so no fix needed — just recorded
  so nobody adds a dependency on it later.
- Checkout Standard (JS widget, `Korapay.initialize({...})`) — client-
  side embed, not directly relevant to this backend's own server-side
  calls, but confirms the same `reference`/`amount`/`currency` shape
  and that `notification_url` can also be passed there.
- Virtual Bank Accounts (NGN) — **entirely separate product from the
  `bank_transfer` channel on `charges/initialize`**, and **not
  implemented anywhere in this repo.** Creates a *persistent*,
  reusable account per customer (`POST /api/v1/virtual-bank-account`,
  fields: `account_name`, `account_reference`, `permanent` (must be
  `true`), `bank_code` (list: Wema `035`, Fidelity `070`, Globus
  `103`, UBA `033`, Moniepoint `090405`, Optimus `107`, Parallex
  `104`, FCMB `214`; use `000` in sandbox), `customer: {name,
  email?}`, and — **mandatory since 2024-01-26** — `kyc: {bvn
  (required), nin (optional)}`. Limited to 50 accounts by default
  (raise via support@korapay.com). Query: `GET
  /api/v1/virtual-bank-account/:accountReference`. Transaction
  history: `GET /api/v1/virtual-bank-account/transactions?account_number=...`.
  Sandbox test-credit: `POST /api/v1/virtual-bank-account/sandbox/credit`
  (NGN only, min 100 / max 10,000,000). USD and KES variants exist as
  separate doc pages (`virtual-bank-accounts-usd`,
  `accepting-payments-with-kes-virtual-bank-account`) — not
  individually fetched this session; same "entirely unimplemented"
  status applies. Flagging for Task 0's d-1 (white-label checkout):
  if the orchestration layer ever wants persistent per-customer pay-
  in accounts rather than one-off checkout sessions, this is a whole
  separate integration, not a byproduct of the existing charges flow.
- Mobile Money, Card Payments (API-driven, not checkout-widget),
  "Pay with Bank (Instant EFT)" (ZAR-only) — each has its own guide
  page (`mobile-money-apis`, `accepting-card-payments-with-apis`,
  `accept-flexible-card-payments-with-api`, `pay-with-bank-instant-eft`)
  but this repo only ever reaches these payment methods indirectly,
  via the `channels`/`default_channel` array on `charges/initialize`
  — it never calls a dedicated per-method endpoint. That's consistent
  with how the repo is built today (one initialize call, Korapay's
  own checkout handles method selection) and not a gap unless a
  future task specifically wants server-side card tokenization
  (`accepting-card-payments-with-apis` requires PCI-DSS Level 1
  certification and AES-256 payload encryption — a materially
  bigger compliance lift than anything else in this audit, worth
  flagging on its own if it's ever considered).
- Underpayments/overpayments on bank-transfer pay-ins — a real
  documented behavior (`handling-underpayments-and-overpayments`)
  this repo has no logic for at all (it only checks `data.status`,
  never compares `amount` vs `amount_paid`). Not fetched in full detail
  this session — flagged as a real, not-yet-scoped gap for whichever
  task first turns on bank-transfer collections at production volume.

*Payouts*
- `POST /api/v1/transactions/disburse` (single payout) —
  **re-confirmed**, matches `processPayout()`'s endpoint exactly.
  Full field table now captured directly from
  developers.korapay.com/docs/payout-via-api, and it's materially
  bigger than what this repo implements:
  - `reference` must be **≥5 characters** — this repo's
    `generateReference()` output is always far longer, so no risk in
    practice, just recorded as a real documented constraint.
  - `destination.amount`: documented as "in two decimal places" —
    worth a real sandbox check before assuming this must differ from
    the base-unit Number this repo already sends; every worked
    example in the docs still shows amount as a plain Number in the
    request and a `"100.00"`-style **string** only in the *response*,
    consistent with "decimal precision," not "send it pre-formatted
    as a string."
  - `destination.bank_country` — **required whenever currency is USD
    or GBP**. Not sent anywhere in this repo's payload.
  - A long list of fields **required only for USD/GBP payouts** —
    `bank_account.bank_name`, `beneficiary_type` (`individual` /
    `corporate`), `first_name`, `last_name`, `business_name` (if
    corporate), `account_type` (`savings`/`checking`),
    `account_number_type` (`account_number`/`iban`), `payment_method`
    (`AbaRouting`/`BicSwift` for USD, `SortCode`/`IBAN` for GBP),
    `routing_number`, `intermediary_routing_number` (if `BicSwift`),
    a full `address_information` object (country/city/state/zip_code/
    street/full_address), and `supporting_documents[]` (title +
    file_reference, uploaded via the separate Supporting Documents
    endpoint) — **none of this exists in `processPayout()` at all**.
    This repo's payout support is real and correct for NGN/KES/ZAR
    bank accounts, but USD/GBP bank payouts would fail outright today
    (missing required fields), not just format the amount wrong.
  - `destination.purpose_of_payment` — optional, but conditionally
    required for some countries per `payout-utilities`' "Get Payment
    Purposes by Country Code" endpoint (US, GB confirmed listed).
    Not sent anywhere in this repo.
  - `metadata` and `notification_url` (per-payout webhook override) —
    same two genuinely-unused-but-available fields as on the charges
    side, same reasoning for why the omission is intentional here too.
  - Response: `{status, message, data: {amount, fee, currency,
    status, reference, narration, message, customer, metadata}}` —
    matches what `processPayout()` already reads.
- **🐛 Real bug found this session, not previously flagged:**
  `processPayout()`'s handling of mobile-money payouts. The code sets
  `destination.type = 'mobile_money'` when `data.payment_method ===
  'mobile_money'`, but then **still only ever populates
  `destination.bank_account: {bank, account}`** — it never builds a
  `destination.mobile_money: {operator, mobile_number}` object at
  all. Korapay's own docs are explicit: `destination.mobile_money` is
  "**Required** — if destination.type is `mobile_money`" and
  `destination.bank_account` is required only for the `bank_account`
  type — sending `bank_account` fields for a `mobile_money`-typed
  request doesn't just omit something optional, it's the wrong nested
  object entirely and would be rejected by Korapay's API. This is the
  same *class* of bug Task 42 already found and fixed for the
  bank-account case (flat payload instead of nested `destination`) —
  it just wasn't caught for the mobile-money branch at the time
  because mobile-money payouts hadn't been exercised yet under the
  "Korapay only" narrowed focus. Documentation-only session — **not
  fixed here**, flagged for the next implementation session.
- **🐛 Second bug found this session:** there is no code anywhere in
  this repo that calls the Mobile Money Operator List endpoint (see
  below), so even once the bug above is fixed, there's currently no
  way for this repo to obtain the `operator` slug (e.g.
  `safaricom-ke`, `mtn-gh`) that `destination.mobile_money.operator`
  requires — it would have to be hardcoded or accepted as-is from the
  caller with no validation against Korapay's real, current list.
- `POST /api/v1/transactions/disburse/remittance` (payout for
  remittance-registered merchants — sender KYC-style fields:
  `remittance_data.sender_name/phone/dob/country_iso/nationality/
  id_type/id_number/service_provider_name/remittance_purpose/
  sender_recipient_relationship/sender_occupation`) — **not
  implemented in this repo**, and likely out of scope unless the
  product owner confirms this account is remittance-registered.
  Flagging one small inconsistency in Korapay's own docs as-is: this
  endpoint is written **without** the `/merchant` prefix every other
  endpoint on the same page uses (`{{baseurl}}/api/v1/transactions/
  disburse/remittance` vs. everything else's
  `{{baseurl}}/merchant/api/v1/...`) — could be a genuine different
  route or a typo in Kora's own docs; not re-derived, worth one real
  sandbox call before ever building against it.
- `GET /api/v1/transactions/:reference` (fetch/verify a single
  payout) — **now directly confirmed**, not just pattern-matched.
  The prior audit's `verifyPayout()` carried an explicit comment
  flagging this path as "a strong pattern-match, not a directly-
  quoted string." This session found it stated outright, with worked
  success/failure examples, on developers.korapay.com/docs/bulk-payouts-via-api
  under "Fetch Payout Transaction" — same path this repo already
  uses. That comment in `providers/korapay.js` can be updated to
  "confirmed" the next time that file is touched (not changed this
  session — docs only).
- `POST /api/v1/transactions/disburse/bulk` (bulk payout) — endpoint
  and full field table confirmed
  (developers.korapay.com/docs/bulk-payouts-via-api): `batch_reference`
  (5-50 chars), `description`, `merchant_bears_cost` (defaults
  **`false`** here — note this is the *opposite* default from the
  single-payout endpoint's `true`), `currency`, `payouts[]` (2-50
  items, each with `reference` (5-50 chars), `amount`, `type` (only
  `bank_account` accepted for bulk — **mobile money is not available
  for bulk payouts**, only for single), `narration`, `bank_account:
  {bank_code, account_number}`, `customer: {name, email}`). Companion
  endpoints: `GET /api/v1/transactions/bulk/:batch_reference` (batch
  status: pending/failed/complete, with per-status counts) and `GET
  /api/v1/transactions/bulk/:batch_reference/payouts` (paginated list
  of every payout in the batch). **None of this is implemented in
  this repo** — Task 0's a-1 scope should treat bulk payouts as a
  net-new build, not an extension of the existing single-payout code.
- Payout Utilities (developers.korapay.com/docs/payout-utilities):
  - `GET /api/v1/misc/payout-payment-purpose-by-country-code/:countryCode`
    — payment purposes for USD/GBP-style payouts. Not implemented.
  - `GET /api/v1/misc/payout-countries-by-currency-code/:currencyCode`
    — supported bank countries for a currency. Not implemented. (This
    is the page with the public-vs-secret-key auth inconsistency
    noted above.)
  - `GET /api/v1/misc/banks?countryCode=NG|KE|ZA` (list banks) and
    `GET /api/v1/misc/mobile-money?countryCode=KE|GH` (list mobile
    money operators) — **🐛 third bug found this session:** this
    repo's `getBanks()` calls `${this.baseUrl}/api/v1/banks?currency=...`
    — **wrong path** (`/api/v1/banks` vs. the documented
    `/api/v1/misc/banks`) **and wrong query param** (`currency=NGN`
    vs. the documented `countryCode=NG`, a country code, not a
    currency code). This looks like exactly the kind of guessed,
    never-verified endpoint the other two provider files' bugs
    turned out to be — genuinely worth a real sandbox call before
    trusting `getBanks()` at all; it may simply 404 or 400 today.
    The mobile-money-operator-list endpoint has no code counterpart
    at all (see the mobile-money payout bug above).
  - `POST /api/v1/misc/banks/resolve` (bank account name resolve,
    NG/KE) and `POST /api/v1/misc/mobile-money/resolve` (mobile money
    account name resolve, GH) — both optional-but-recommended
    pre-payout verification steps, both unimplemented here.
  - `POST /api/v1/payouts/availability` (bank/MMN availability check,
    **South Africa only**) — unimplemented, low priority given the
    narrow scope.
- `GET /api/v1/payouts` (Payout History — list, not per-reference;
  filters: currency, date_from/to, limit, starting_after/ending_before)
  — unimplemented.
- Payout error handling: Korapay's own docs are explicit and this
  repo already follows the principle correctly for the single-payout
  path — 502/504/503/500 and other unexpected errors must NOT be
  treated as a failed payout; always verify via the query endpoint
  before giving value, since the payout may have actually gone
  through despite the error response. `processPayout()`'s own code
  comments already reflect this correctly.
- Supported payout destinations, per developers.korapay.com/docs/send-payments
  (dated 2026-08-15, newer than the prior audit's currency list —
  **superseding it, not just adding to it**): NGN/KES/ZAR **bank
  accounts**; KES/GHS/XOF/XAF/EGP/TZS **mobile money**; USD/GBP
  **bank accounts** (with the extra required-field set above); and
  **stablecoins (USDC and USDT)** — this last one is genuinely new
  information, not mentioned anywhere in the prior audit, and this
  repo has zero stablecoin-payout support of any kind (no destination
  type, no code path, nothing in `processPayout()`'s `type` ternary).

*Refunds — 🆕 entire product surface, zero implementation in this repo*
  (developers.korapay.com/docs/refunds-api, fetched fresh this
  session — the prior audit didn't cover this at all):
- `POST /api/v1/refunds/initiate` — `payment_reference` (required,
  the *original charge's* reference) + `reference` (required, a
  **new**, merchant-generated reference *for the refund itself*, ≤50
  chars) + optional `amount` (omit for a full refund), `reason`
  (≤200 chars), `webhook_url` (per-refund override, ≤200 chars).
  NGN minimum refund amount: 100.
- `GET /api/v1/refunds/:reference` — refund details by the refund's
  own reference (not the original payment's reference).
- `GET /api/v1/refunds` — list, filterable by currency/date_from/
  date_to/limit/starting_after/ending_before/status
  (`processing`/`failed`/`success`).
- Webhook events `refund.success`/`refund.failed` were already
  recorded in the prior audit's webhook section, but **one field
  meaning was wrong/incomplete there**: the refund webhook's `data.reference`
  is the **refund's own** reference, while `data.payment_reference`
  is the **original payment's** reference — these are two different
  values, not the same value under two names (contrast with the
  regular charge-success webhook, where `reference` and
  `payment_reference` are the same value, with `payment_reference`
  marked deprecated there). Anyone implementing refund webhook
  handling needs to key off `payment_reference` to find the original
  transaction, not `reference`.

*Chargebacks — 🆕 entire product surface, zero implementation*
  (developers.korapay.com/docs/chargebacks): `GET
  /api/v1/chargebacks/:reference` (details), `GET /api/v1/chargebacks`
  (list, filterable), `PATCH /api/v1/chargebacks/:reference` (mark
  Won/Lost/Partial — declining or partially declining requires PDF
  evidence uploaded via the separate Supporting Documents endpoint
  with purpose `chargeback_supporting_document`). Webhooks fire on
  chargeback creation and on Won/Lost/Partial resolution — event
  names not independently confirmed this session. **One base-URL
  inconsistency worth flagging as-is:** every chargeback endpoint on
  this page is written as `https://api.korapay.com/api/v1/chargebacks/...`
  — **missing the `/merchant` segment** every other endpoint family
  in this entire audit uses. Same caveat as the remittance-payout
  path above: could be a real, deliberately different route, or a
  documentation typo — not re-derived, confirm with one real sandbox
  call before building against it.

*Balance*
  (developers.korapay.com/docs/balance-api,
  developers.korapay.com/docs/balance-history-api): `GET
  /api/v1/balances` — returns `{available_balance, pending_balance}`
  per currency (NGN always present; USD/GHS/KES/XAF/XOF/ZAR for
  multi-currency accounts; USD additionally carries an
  `issuing_balance` field, presumably tied to Card Issuing). `GET
  /api/v1/balances/history` — ledger of everything that moved the
  balance (pay-ins, payouts, chargeback deductions, etc.), with
  `direction` (debit/credit) and `source` (e.g. `"chargeback"`) per
  entry. **Neither is implemented in this repo.** Relevant beyond
  just bookkeeping: a pre-payout balance check would directly answer
  "do we actually have enough to disburse this" before hitting the
  disburse endpoint and discovering insufficient funds the hard way —
  worth considering for Task 0's b-1 routing-rule design (routing
  could account for available balance per provider, not just
  currency/country/method).

*Currency Conversion* (developers.korapay.com/docs/exchange-rate-api,
  /currency-conversion-api, /dynamic-currency-conversion): this
  repo's use of DCC is real but partial — `processPayment()` already
  sends `payment_currency`/`settlement_currency` together when the
  caller supplies both (confirmed correct in the prior audit). The
  *separate* standalone Currency Conversion API — look up an exchange
  rate for a pair, then `POST` to initiate an actual balance-to-
  balance conversion (moving funds from one currency balance to
  another, independent of any specific transaction) — is a different
  feature and **not implemented here at all**. An email confirmation
  is sent on completion per Kora's own docs; conversions require
  sufficient available balance in the source currency (ties back to
  the Balance API gap above).

*Split Payments* (developers.korapay.com/docs/split-payments) — page
  exists, not fetched in full detail this session (lower priority —
  no indication anywhere in Task 0 or this repo's existing code that
  splitting a single payment across multiple recipients is a near-
  term need). Recorded as a known, unaudited gap rather than silently
  skipped.

*Webhooks* — prior audit's HMAC-SHA256-of-`data`-only finding,
  Buffer/timing-safe-equal implementation, and the six core event
  names all **re-confirmed**, no changes. New from this session's
  direct re-fetch of developers.korapay.com/docs/webhooks:
  - The "Resend Webhook" dashboard button only activates under
    specific conditions: for payouts, channel `api` + status
    `successful`/`failed`; for pay-ins, channel `api` + status
    `successful`/`failed`, **or** channel `modal` + status
    `successful` only (a pending/failed modal pay-in can't be
    manually resent). Operationally relevant for the product owner,
    not something this repo's code needs to handle.
  - Best-practice guidance directly from Kora: keep track of every
    notification received and check it hasn't already been processed
    before giving value (i.e., their own docs assume the merchant
    does idempotency — directly relevant to Task 0/c-1's resolution
    elsewhere in this file) and always acknowledge with a bare `200`
    before doing further processing, to avoid a timeout-triggered
    retry.
  - Sample payloads for **every** event family were captured directly
    this session (single payout, bulk payout, NG VBA pay-in, card/
    bank-transfer/mobile-money pay-in, refund) — all consistent with
    the six-event-type/`data`-shape findings already on file; no
    corrections needed there beyond the `payment_reference` nuances
    already called out above under Pay-ins and Refunds.

*Not covered by this pass — real, explicit exclusions, not oversights:*
  Card Issuing (virtual card creation/funding/withdrawal/management,
  beta), Direct Debit (authorization creation/retrieval, variable-
  authorization debits), Identity/KYC & KYB (NG/ZA/GH/KE/US/CI, BVN/
  NIN/vNIN/SSN/passport/national-ID/phone verification, liveness
  check, document verification), Payment Links, Pool Accounts,
  Voucher Payments (checkout + API), Settlements & Audit Logs, and the
  WooCommerce plugin. None are referenced anywhere in this repo's
  code or in Task 0's stated scope. Also worth recording precisely
  because it surprised this session: developers.korapay.com's own
  `llms.txt` index (the file explicitly meant to help an AI agent
  discover all pages) **does not list every page the live site's own
  sidebar shows** — Errors, Payment Links, Bank Transfers, Pool
  Accounts, a Card Payments overview page, Pay with Bank, Voucher
  Payments, Pay-ins History API, Withdrawals, all of Direct Debit,
  all of Card Issuing, all of Identity, all three Balance pages, and
  all three Settlements pages are missing from `llms.txt` but present
  in the sidebar fetched directly from developers.korapay.com/docs/balance-api
  this session. A future discovery pass on any of the excluded areas
  above should re-fetch the live sidebar rather than trusting
  `llms.txt` alone to enumerate what exists.

**JuicyWay — FULL API discovery pass, re-audited 2026-09-06 (supersedes
the endpoint-path/auth/payload/error material below; the webhook
section below this one is unchanged/re-confirmed, not superseded —
nothing was dropped, only expanded).** This pass resolves Task 8b,
which had sat open since Task 15 flagged the endpoint path as
genuinely unverified. Every source below was fetched directly this
session from docs.juicyway.com (via its `.md`-suffixed pages —
confirmed again this session that the bare paths often don't resolve
standalone, go through the page's own `.md` URL or `llms.txt`'s index)
— Home, Payment Initialization overview, Card Payment Initialization,
Authentication, Errors, Fetch Payment, List Payments. **Four real,
confirmed bugs came out of this pass, none fixed here since this is a
doc-research pass per the Discovery Convention** — each queued as its
own task below (Tasks 45a–45d), plus one real unresolved ambiguity
(Task 45e) that blocks adding JuicyWay to `CONFIRMED_PROVIDER_CURRENCIES`
until resolved, not just an omission the way it's listed today.

*Endpoint path — CONFIRMED WRONG, this is a real bug, not a
docs-mismatch curiosity*
- The real endpoint, per docs.juicyway.com/payments/initialize-payment/cards.md,
  is **`POST /payment-sessions`** — not `/v1/charges`, which is what
  `providers/juicyway.js#processPayment` has called since this file
  was first written (the `⚠️ Verify exact endpoint path in Juicyway
  docs` comment has sat on this exact line since Task 5). `/v1/charges`
  does not appear anywhere in JuicyWay's documented API surface — it
  looks like a Paystack-shaped path copied across providers rather
  than anything JuicyWay itself ever documented.
- The verify/fetch endpoint is **`GET /payments/{id}`**
  (docs.juicyway.com/payment-transactions/fetch-payment.md) — not
  `/v1/charges/${reference}`, which `verifyTransaction` currently
  calls. This is wrong on both the path *and* the identifier: see the
  "Reference vs. ID" finding below — `{id}` here is JuicyWay's own
  UUID for the payment, not the merchant-supplied `reference` this
  repo currently passes into that URL slot.

*Authorization header — CONFIRMED WRONG, a real bug*
- docs.juicyway.com/authentication.md states explicitly: **"Authorization
  headers should be in the following format: `Authorization: API_KEY`"**
  — the raw key, with no scheme prefix. Every code sample across the
  docs (cURL, Node.js, Python) confirms this: `'Authorization': '
  YOUR_API_KEY'`, never `Bearer YOUR_API_KEY`. `providers/juicyway.js`
  currently sends `'Authorization': \`Bearer ${this.apiKey}\`` in both
  `processPayment` and `verifyTransaction` — this is a **confirmed,
  concrete bug**: every real API call this repo makes to JuicyWay
  today would fail authentication with a 401, not because the key is
  wrong but because the header format is. This directly resolves
  Task 44's previously-unverified "auth-header-prefix... claims
  unverified" note — now verified, and verified wrong.

*Request payload shape — CONFIRMED INCOMPLETE, a real bug*
- The documented required body for `POST /payment-sessions` is a
  **deeply nested** object: top-level `amount`, `currency`,
  `description` (≤200 chars), `reference` (≤50 chars), `payment_method:
  { type: "card" }`, `order: { identifier, items: [{ name, type:
  "digital"|"physical" }] }`, and a `customer` object requiring
  `email`, `first_name`, `last_name`, `phone_number` (E.164),
  `billing_address` (object), `type` (`business`|`individual`), and
  `ip_address` (IPv4) — all marked `required` in the docs.
  `providers/juicyway.js#processPayment` currently sends a **flat**
  `{ amount, email, reference, currency }` — missing `description`,
  `payment_method`, `order`, and the entire nested `customer` object
  (only `email` survives, and not even nested correctly). Combined
  with the endpoint-path and auth-header bugs above, a real call as
  currently coded would fail even if those two were fixed in
  isolation — this repo's caller-facing `processPayment(data)`
  contract would need real rework (accepting/requiring the extra
  fields) before this endpoint could work at all, not just a URL and
  header fix. This directly resolves the other half of Task 44's
  previously-unverified "payload-completeness claims unverified" note
  — now verified, and verified incomplete.

*Response shape & the reference-vs-ID distinction — new finding, not
previously documented*
- A successful `POST /payment-sessions` call returns `{ data: {
  status, auth_type, expires_at, links, message, payment: { id,
  amount, currency, status, customer, order, payment_method,
  reference, date, description, mode, cancellation_reason } } }` —
  note the **payment's own JuicyWay-assigned `id`** (a UUID) lives at
  `data.payment.id`, separate from and different in shape from the
  merchant-supplied `reference` this repo generates via
  `generateReference('juicyway')`, which JuicyWay echoes back
  unchanged at `data.payment.reference`.
- **Real architectural finding:** `GET /payments/{id}` (Fetch Payment)
  takes JuicyWay's own `id`, not the merchant's `reference` — and
  List Payments' documented query filters (`status`, `before`,
  `after`, `limit`, `created_after`, `created_before`) include **no
  filter-by-`reference` option**. So there is no documented way to
  look a payment up by the merchant-generated reference alone — a
  caller must capture and persist `data.payment.id` from the
  initialize response and use *that* for any later verify call. This
  repo's `verifyTransaction(reference)` signature currently assumes
  reference-based lookup works (matching Paystack/Korapay's own
  reference-based verify calls) — for JuicyWay specifically, that
  assumption doesn't hold against the documented API, a real,
  provider-specific difference worth designing around deliberately
  rather than discovering at runtime.

*Error response format — CONFIRMED WRONG, a real bug, same class as
Paystack's Task 8c finding but with a different failure mode*
- Confirmed at docs.juicyway.com/errors.md: every JuicyWay error is
  `{ error: { code, message, type, details } }` (validation errors
  additionally carry an `errors: [{ field, message }]` array instead
  of/alongside `details`). The message a caller should show a user
  lives at **`error.message`**, nested — not top-level.
  `providers/juicyway.js` currently does
  `responseData.message || 'Juicyway payment failed'` (and the
  equivalent in `verifyTransaction`) — `responseData.message` is
  `undefined` for every real JuicyWay error response, since the real
  field is `responseData.error.message`. **Unlike Paystack's Task 8c
  bug (which surfaces a technically-true-but-misleading success
  message), this bug silently swallows JuicyWay's real, specific
  error text on every single failure** and always falls back to the
  generic hardcoded string instead — worse for debugging and for
  end-user-facing error messages alike, since JuicyWay's own
  documented messages (e.g. "Amount must be at least 100000",
  "Currency must be one of: NGN, USD, CAD") are exactly the kind of
  specific, safe-to-surface text Task 13's precedent already treats
  as fine to show users for other providers.
- HTTP status codes confirmed: 200, 201, 204, 400, 401, 403, 404, 422,
  429, 500 — plus a **402** for card declines specifically (documented
  separately on the Card Payment Initialization page, not the general
  Errors page: `card_declined` with reasons like insufficient funds,
  suspicious activity, expired card) — a status code Paystack/Korapay
  don't use this way in this repo's existing integrations, worth
  handling explicitly rather than falling into a generic
  catch-all-non-2xx branch if user-facing decline messaging matters.
- Rate limits are **documented per-endpoint, not account-wide**:
  Fetch Payment is 100 req/min per key; List Payments is 1,000 req/hour
  with a 100 req/min burst cap and max 100 records/page. No rate limit
  was documented specifically for `POST /payment-sessions` itself in
  the pages fetched this session. `429` responses use the same
  `{ error: { code: "rate_limit_exceeded", message, type,
  retry_after } }` envelope, with a `retry_after` (seconds) field —
  distinct from Paystack's header-based rate-limit signaling.

*Currencies — a real, three-way documented inconsistency, not a
missing-entry problem like Paystack's XOF finding*
- **Three different currency lists appear across JuicyWay's own primary
  docs, none of which fully agree:**
  1. `payments/overview.md`'s "Supported Currencies" section lists
     only **NGN, CAD**.
  2. `payments/initialize-payment.md` and
     `payments/initialize-payment/cards.md`'s own `currency` parameter
     docs both list **NGN, USD, CAD, USDT, USDC**.
  3. The very same cards.md page's own **422 error example** says
     *"Currency must be one of: NGN, USD, CAD"* — excluding the two
     stablecoins the parameter docs just said were supported, on the
     same page.
- **Not resolved here.** No entry has been added to
  `CONFIRMED_PROVIDER_CURRENCIES.juicyway` this pass — `getAmountFormat`
  already throws for `juicyway`/`payscribe` today specifically to avoid
  a silent wrong guess on real money, and this three-way conflict is
  exactly the situation that guard exists for. Whichever session
  eventually implements JuicyWay for real should resolve this against
  a live sandbox call (attempt a USDT/USDC session and see whether it's
  accepted or 422s) rather than picking one of the three documented
  answers — same spirit as the DodoPayments currency-conflict finding
  above, applied to a same-provider three-way conflict instead of a
  two-source one.
- Separately, minimum-amount documentation for cards is also
  internally inconsistent on the very same cards.md page: the
  parameter docs say "Minimum: 100" (in minor units — i.e. ~1.00 in
  major-unit terms) but the page's own 400 error example says *"Amount
  must be at least 100000"* — a thousand-fold difference. Also
  unresolved here, flagged for the same live-sandbox-check treatment
  as the currency conflict, not guessed at.

**JuicyWay — base URL, and webhook scheme previously confirmed
2026-08-27, unchanged and re-confirmed relevant this session (not
re-verified line-by-line, no new source fetched for this part —
nothing above touches webhooks):**
- Real, current docs: **https://docs.juicyway.com** (confirmed to
  exist and be current). There's also **https://docs.spendjuice.org**,
  which appears to be a *different, newer* product surface from the
  same company (card issuing, USDC wallets) — don't assume the two
  document the same endpoints. This repo's base URL
  (`api-sandbox.spendjuice.com` / `api.spendjuice.com`, set in
  `utils/helpers.js`) matches what docs.juicyway.com/home itself shows
  as its two `Development Environments` code samples — confirmed
  directly, this base URL was already correct.
- The existing `providers/juicyway.js` has an explicit
  `⚠️ Verify exact endpoint path in Juicyway docs` comment on the
  `/v1/charges` call — **now resolved by this session's full audit
  above**: the path is confirmed wrong (`/payment-sessions` is
  correct), see the "Endpoint path" finding above rather than treating
  this as still-open.
- Webhook scheme: **confirmed directly** against
  docs.juicyway.com/webhooks.md (fetched 2026-08-27, via
  docs.juicyway.com/llms.txt's page index — the `/webhooks` path alone
  404s or isn't independently fetchable, use the `.md` suffix or go
  through llms.txt). Materially different from both Paystack's and
  Korapay's schemes, in three ways: (1) there is **no signature HTTP
  header at all** — the checksum travels *inside* the JSON body as a
  `checksum` field alongside `event`/`data`; (2) the HMAC key is the
  merchant's **"business ID"**, a separate credential from the secret
  API key used for REST calls — this repo has no env var for it yet,
  so `providers/juicyway.js` now reads `JUICYWAY_BUSINESS_ID` directly
  (not added to `getProviderKey()`'s public/secret map since it
  doesn't fit that shape); (3) the signed string is
  `${event}|${json_encoded_data}` where `data` must be JSON-encoded
  with **keys in alphabetical order at every nesting level** — the
  docs explicitly warn about this and show alphabetized nested example
  payloads. Plain `JSON.stringify()` does not do this (insertion order,
  not alphabetical), so a local `stableStringify()` helper was added
  to `providers/juicyway.js` to match. Notably, Juicyway's own Node.js
  doc example imports `json-stable-stringify` but then never calls it
  — it uses plain `JSON.stringify(data)` in the actual `validateSignature`
  code shown — which looks like a bug in their own sample; implemented
  to match the explicitly documented alphabetical-order requirement
  instead of that inconsistent sample. Digest is hex, **uppercase**:
  the docs' own sample checksum value is uppercase hex and the
  Python/Node examples both explicitly uppercase their digest, though
  the PHP example lowercases both sides before comparing instead (a
  cross-language inconsistency in Juicyway's own docs) — implemented
  as uppercase-with-tolerant-comparison (the incoming checksum is also
  uppercased before comparing), so a lowercase sender still verifies.
  Verified the whole scheme numerically this session with a throwaway
  script: valid checksum accepted, tampered data rejected, missing
  checksum rejected, wrong business-ID rejected, checksum is
  independent of the *sender's* top-level key order (since
  `stableStringify` re-sorts regardless), and a lowercase-hex checksum
  from a sender still verifies — all six passed, script deleted after.
  Only one documented event pair exists so far:
  `payment.session.succeeded` / `payment.session.failed`, both sharing
  one payload shape with `data.status` = `success`/`failed`. Docs also
  note **"In sandbox, successful transactions remain pending. Only
  failure events are sent"** — relevant for Task 14's manual test pass,
  since the success path can't be exercised via a real sandbox webhook.
  This repo now implements `POST /api/webhooks/juicyway` (Task 5):
  verifies the checksum (401 on failure/missing), logs both event
  types with reference/amount/currency/status (no persistence layer
  yet — see Task 12).

**Remita — FULL API discovery pass, audited 2026-09-06 (new — resolves
a-6, doc-research only, no code — `providers/remita.js` does not exist
yet).** Unlike every other provider audited in this file, Remita has
**no single public developer-docs site** (no `docs.remita.net`
equivalent to `paystack.com/docs`, `docs.juicyway.com`, or Korapay's
own docs). What's publicly findable is a patchwork of: one official
SystemSpecs-authored PDF ("Remita Developer Documentation") hosted on
a third-party document site rather than remita.net itself; several
official `RemitaNet`-org GitHub SDK repos (Node.js, Python, Java, PHP)
for Remita's **Billing Gateway** product specifically, a different
surface from the general payment-collection RRR gateway; one public
Postman collection (documenter.getpostman.com, owner id 3613789)
covering FIRS-tax-specific RRR integration; and a mix of
third-party/community material (a WordPress/WooCommerce plugin, older
standalone PHP/JS libraries, blog walkthroughs) that documents
real-world behavior but isn't Remita's own authoritative source. **Per
the Discovery Convention above, this is exactly the "genuinely can't
be found" case** — this session did not stop at that, since enough
converging detail exists across official SDKs/PDF/Postman collection
to write a real audit, but the product owner should still be asked
directly for current merchant onboarding docs and sandbox credentials
before implementation starts, rather than treating what's below as
equivalent in authority to Paystack's or JuicyWay's own docs sites.

*Two structurally different integration surfaces exist — do not
conflate them*
- **Surface 1 — the classic RRR Payment Gateway** (what this repo
  almost certainly wants, matching how Korapay/Paystack/JuicyWay are
  used here): merchant generates a Remita Retrieval Reference (RRR)
  for an amount, the payer settles it (card, bank transfer, USSD, or
  in-branch), the merchant checks/confirms status by RRR.
- **Surface 2 — the Billing Gateway** (the `RemitaNet` org's Node/
  Python/Java SDKs): a *different* product where the merchant's own
  customers pick a **biller** (school, church, government MDA, etc.)
  already onboarded to Remita and pay *that biller* through the
  merchant's platform — oriented around `GetBillerList`,
  `GetRRRDetails(billId)`, `ValidateRequest`, not a generic
  "charge this amount" call. **Confirmed real risk:** these two
  surfaces are easy to conflate because both produce and consume an
  "RRR," but they are different products with different credential
  sets (Billing Gateway uses a separate "Public key/Secret key" pair
  from the Billing page of a merchant's dashboard, not the classic
  `merchantId`/`apiKey` pair). This audit is written for **Surface 1**
  only, since that matches this repo's `processPayment(data)` /
  `verifyTransaction(reference)` shape used by every other provider —
  Surface 2 would need its own separate discovery pass and its own
  provider file if the product owner actually wants biller-payment
  functionality, not charge-collection.

*Base URLs — CONFIRMED INCONSISTENT across primary sources, a real
unresolved ambiguity, same class of finding as JuicyWay's currency
conflict*
- The public FIRS-integration Postman collection states test-mode RRR
  generation goes to `https://remitademo.net/remita/exapp/api/v1/send/api`
  while test-mode status-checking goes to a **different host entirely**,
  `https://login.remita.net/remita/ecomm` — i.e. even within one
  official source, generate and verify are documented against two
  different base hosts in test mode, not variations of the same host.
  The same collection's live-mode block points **both** operations at
  `login.remita.net`, with credentials "provisioned upon successful
  UAT" rather than published.
- A separate official `RemitaNet` GitHub SDK sample (`remita-rrr-generator-status-dotnet`) instead
  defines its own demo/live constants as `https://demo.remita.net`
  (note: no "remita" repeated in the hostname, unlike the Postman
  collection's `remitademo.net`) and `https://login.remita.net`, with
  its own generate-RRR path
  (`/remita/exapp/api/v1/send/api/echannelsvc/merchant/api/paymentinit`)
  that matches neither the Postman collection's path nor the
  classic-PHP-library pattern below.
- Multiple older PHP/JS community integrations instead call
  `www.remitademo.net/remita/ecomm/...` paths ending in `.reg`
  (`init.reg`, `status.reg`, `orderstatus.reg`) — a third, older-looking
  URL family again distinct from the two above.
- **Not resolved here.** Three source families (official Postman
  collection, official .NET SDK sample, community `.reg`-style
  integrations) each imply a different combination of hostname and
  path for what is nominally the same "generate RRR" operation.
  Whichever session implements Remita for real should get the current
  base URL and path directly from the product owner's own registration/
  demo-credential email (the FIRS collection itself says these are
  sent by email on request, not published), rather than guessing which
  of the three documented shapes is current — this is a case where
  picking wrong doesn't 422 cleanly, it likely just times out or 404s
  against a stale host.

*Authentication — CONFIRMED, but two incompatible schemes coexist and
picking the wrong one for a given base URL will silently fail*
- **Classic/legacy scheme (matches the `.reg`-style and .NET-sample
  URL families above):** no `Authorization` header at all — the hash
  itself is embedded in the URL path or POST body. Two distinct hash
  formulas, confirmed identical across three independent sources (an
  official PDF excerpt, a community PHP library, and a community React
  walkthrough): **RRR generation** hashes
  `SHA512(merchantId + serviceTypeId + orderId + amount + apiKey)`;
  **status/verification** hashes a *different* concatenation order,
  `SHA512(RRR + apiKey + merchantId)` — these are not interchangeable,
  and re-using the generation hash formula for a status check (or vice
  versa) will produce a hash that simply doesn't match, with no
  explanatory error beyond a generic auth failure.
- **Newer REST/JSON scheme (matches the Postman-collection URL
  family):** a single `Authorization` header of the form
  `remitaConsumerKey={merchantId},remitaConsumerToken={hash}` — same
  two hash formulas as above (generation vs. status use different
  concatenations), just carried in a header instead of the URL, with a
  JSON body instead of a query string/form POST.
- **Confirmed NOT part of Remita's own generic merchant API, despite
  looking like a real Remita integration at first glance:** a
  `Nau-Contractor-ID` / `Nau-Api-Key` / `Nau-Access-Token` header
  scheme found in one source is explicitly a single university's own
  wrapper built on top of Remita for its own contractors/internal
  developers — not a scheme any other Remita merchant would be issued.
  Flagging this explicitly because it's the kind of institution-specific
  wrapper that could get mistaken for Remita's own documented API if
  this session's search results were taken at face value; excluded
  from the scheme options above for that reason.
- **Not resolved here which of the two real schemes (classic-hash-in-URL
  vs. header-based) is current/preferred for new integrations** — the
  Postman collection (header-based) is the most recently-structured
  official source found, but nothing confirms the classic scheme is
  deprecated rather than still-supported-in-parallel. Get this directly
  from the product owner's onboarding material rather than assuming.

*Request/response shape for RRR generation — CONFIRMED core fields,
consistent across every source regardless of which base-URL/auth
family is used*
- Required fields, consistent across all sources: `serviceTypeId`,
  `amount`, `orderId` (merchant-generated, this repo's existing
  `generateReference()` pattern maps onto this), `payerName`,
  `payerEmail`, `payerPhone`, `description`. `merchantId` is either
  sent explicitly in the body (classic scheme) or implied by the
  `remitaConsumerKey` in the header (REST scheme) — not sent in both
  places redundantly in any single source seen.
- Successful response carries the generated `RRR` (a numeric string,
  typically 12 digits in real examples across sources) plus an echo of
  `orderId`/`serviceTypeId`/a generation timestamp — exact field names
  for this vary between sources (`rrr` lowercase vs. `RRR` uppercase,
  `date_generated` vs. no timestamp field at all in the classic-scheme
  examples), another concrete detail to pin down against the specific
  base URL/scheme actually used rather than assumed from whichever
  source happens to be open.

*Status/response codes — CONFIRMED TO EXIST, but meanings are only
partially and inconsistently documented across sources — a real,
unresolved gap, not just a nice-to-have*
- A **numeric `statuscode`** is confirmed to accompany RRR status
  (seen directly in a real merchant redirect URL from a government
  portal's integration: `statuscode=025` paired with a human-readable
  `status=Payment Reference generated`, for a **pending, not yet
  paid** RRR). No source found in this session's search enumerates the
  **full** set of `statuscode` values and their meanings in one place
  — `025` (pending/reference-generated) is the only value this session
  could confirm against a real, dated, in-the-wild example rather than
  a code sample's placeholder text.
- Separately, a **university-wrapper-specific** response envelope
  (not confirmed as Remita's own canonical shape — see the auth-scheme
  caveat above) uses HTTP-style codes instead: `200`/`"RRR generated
  successfully"` on success, `424`/`"RRR could not be generated"` and
  `503`/`"RRR was generated but could not be saved"` on failure. Listed
  here for completeness but **explicitly not confirmed to be Remita's
  own response shape** — it may be that wrapper's own translation
  layer, not what Remita's raw API itself returns. Conflating the two
  would overstate how much of this is actually verified.
- **Not resolved here.** A real sandbox call (once credentials exist)
  is the only reliable way to enumerate the actual `statuscode`
  space and confirm which response envelope (raw Remita vs. some
  integrator's own wrapper shape) this repo would actually receive —
  flagging as a required pre-implementation step, same treatment as
  JuicyWay's currency conflict and DodoPayments' settlement-currency
  conflict above, not something to guess from the fragments found here.

*Confirmation/webhook model — CONFIRMED to be architecturally
different from every other provider this repo integrates, a real
design consideration for b-3's normalized-webhook-envelope work*
- **No inbound HMAC-signed webhook header of any kind was found in any
  source** — unlike Paystack (`x-paystack-signature`), Korapay, or
  JuicyWay (body-embedded `checksum`). The classic flow instead
  redirects the *payer's browser* back to a merchant-supplied
  `responseurl`/`notify_url` with **unsigned** query-string parameters
  (`status`, `RRR`, `orderID` confirmed directly from a real,
  dated government-portal redirect URL found this session). Because
  these parameters travel through the payer's own browser and carry no
  cryptographic signature, they are **not safe to trust as proof of
  payment on their own** — every community integration examined
  (the WooCommerce plugin, the PHP libraries) treats the redirect only
  as a *signal to go re-check status*, then calls the separate
  hash-verified status/verify endpoint (using the `RRR+apiKey+merchantId`
  hash from the Authentication section above) as the actual source of
  truth before crediting anything. **This repo's other four providers
  all verify an inbound signature and trust the payload; Remita's
  documented pattern instead requires an outbound confirmation call
  triggered by an untrusted inbound signal** — a materially different
  shape that Task 0/b-3's per-provider webhook-to-normalized-envelope
  mapping needs to account for explicitly (Remita's "webhook handler"
  would need to *call out* to the verify endpoint before it can decide
  `status`, not just parse and map an inbound body like the other four
  do) rather than assumed to fit the same pattern.
- No separate server-to-server IPN/webhook mechanism distinct from the
  browser-redirect model above was confirmed in any source this
  session found — flagged as a gap to close with the product owner's
  onboarding docs, not assumed absent.

*Currency — CONFIRMED NGN is supported; broader multi-currency support
NOT confirmed either way, an omission worth flagging rather than
guessing*
- Every source examined this session — the RRR examples, the
  government-portal redirect, the demo credentials, the Billing
  Gateway SDKs — deals exclusively in Naira amounts with no currency
  parameter at all in the classic RRR-generation payload (amount is
  sent as a bare number, currency implied rather than stated). No
  source found documents an explicit multi-currency capability or
  parameter for the RRR gateway the way Paystack/Korapay/JuicyWay each
  do. **Not added to `CONFIRMED_PROVIDER_CURRENCIES` this pass** —
  absence of a documented currency parameter is different from a
  confirmed single-currency restriction, and `getAmountFormat`'s
  existing guard (throws rather than guesses) is the right posture
  until the product owner's actual onboarding docs either show a
  currency field or confirm NGN-only.

*Rate limits and error taxonomy — NOT FOUND, an honest gap rather than
an inferred value*
- No source examined this session documents per-endpoint or
  account-wide rate limits for the RRR gateway (contrast with
  JuicyWay's documented per-endpoint limits and Paystack's documented
  limits) — flagged as unknown, not assumed unlimited.
- No general error-code taxonomy comparable to Paystack's or JuicyWay's
  `errors` doc page was found for the classic RRR gateway itself —
  what exists (see the statuscode/response-code finding above) is
  fragments from specific real examples and one wrapper's own
  translation layer, not a documented enumeration from Remita itself.

**Payscribe — FULL API discovery pass, audited 2026-09-06 (first full
pass — resolves the "Waiting on a docs link" placeholder below and
Task 6's docs blocker; `providers/payscribe.js` already exists and is
largely confirmed correct by this pass, a different outcome from the
JuicyWay re-audit above).** Source this time is different in kind
from every other pass in this section: the product owner supplied the
primary source **directly, as a saved-page snapshot of
docs.payscribe.co** (Postman/ReadMe-hosted public docs, page title
"Build, Embed Launch Financial Products in One Single API",
Snapshot-Content-Location `https://docs.payscribe.co/#ff3b4877-f018-4573-9c01-d61e36a679be`),
rather than this session locating it via web search — same
authoritative status as Paystack/JuicyWay's own docs sites, just
delivered differently. **Real caveat on completeness, stated plainly:**
this is a single-page-app doc site, and the saved snapshot only
contains the DOM for whichever sections were expanded/open at save
time. The **sidebar nav is complete** (every resource family and
endpoint name below is real and confirmed to exist), but the actual
request/response body was only captured for: Introduction (incl.
environment, response codes, rate limits), the full Webhook Security &
Signature Verification folder (Overview + PHP/Node.js/Python
verification examples + Common Mistakes), and two Bills Payments
endpoints (Cable TV vend, Data Vending). Every other resource family
visible in the nav — Internet Subscription, ePins, Airtime to Wallet,
Electricity, Bulk SMS, Betting, Airtime, International Bills, My
Account, Payout, Customers, Collections (Wallet System **and** NGN
Virtual Accounts — this is the family `processPayment` actually calls
into), Payment Links, Invoices, FX & Conversions, Card Issuing,
Stablecoin Cards, KYC, Savings, Misc — is confirmed to **exist** by
name/method only; none of their individual endpoint bodies were
captured this pass. Treat every finding below as confirmed against a
primary source, and everything not mentioned below as **still
nav-only**, not silently assumed equivalent to what was captured.

*Environment & authentication — CONFIRMED, and matches this repo's
existing code exactly*
- Base URLs, confirmed at the Introduction page: **Sandbox (test
  mode) `https://sandbox.payscribe.ng/api/v1`**, **Production (live
  mode) `https://api.payscribe.ng/api/v1`** — this is a byte-for-byte
  match with what's already in `utils/helpers.js`'s
  `getProviderBaseUrl('payscribe')` (`development`/`production` pair),
  no change needed there.
- Auth: `Authorization: Bearer {API key}` on every request — confirmed
  directly ("Use your API key as Bearer in the header of each
  request"), matching `providers/payscribe.js`'s existing
  `Authorization: Bearer ${this.secretKey}` exactly. One minor,
  non-blocking inconsistency **in the docs themselves, not in this
  repo's code**: the webhook folder's own auth example shows a secret
  key formatted `ps_sk_test_YOUR_SECRET_KEY`, while the PHP/Node/Python
  verification code samples instead hardcode `sk_live_your_secret_key`
  as their placeholder — two different key-prefix conventions shown
  side-by-side in Payscribe's own docs. Not something to resolve here;
  just don't treat either placeholder's prefix as a confirmed literal
  format.
- Rate limit: **60 requests/second** account-wide, confirmed at the
  Introduction page; exceeding it returns HTTP `429` with body
  `{"message": "API rate limit exceeded"}`. New information — this
  repo has no rate-limiting/backoff handling for Payscribe (or any
  provider) today, same gap already noted for Paystack/DodoPayments
  above, not a Payscribe-specific deficiency.

*Response envelope & status/response codes — CONFIRMED, and explains
why Task 13's existing `.description`-based error parsing is already
correct*
- Every captured example (Cable TV vend, Data Vending, both success
  and the webhook code samples' implied shape) uses one consistent
  envelope: **`{ status: bool, description: string, message: {
  details: {...} }, status_code: number }`**. `providers/payscribe.js`
  already reads `responseData.description` for its error message
  (the Task 13 comment calls this out explicitly) — **confirmed
  correct by this pass**, not a guess that turned out lucky.
- Full response/status-code table, confirmed at the Introduction page
  (this is Payscribe's own `status_code` value, not a raw HTTP status
  in every case — see the 201 gotcha below):

  | Code | Meaning |
  |---|---|
  | 200 | Success |
  | 201 | Transaction Pending — reverify using Payscribe `trans_id` or the `ref` passed |
  | 400 | Bad Request — something missing in the body |
  | 401 | User not authenticated |
  | 403 | Forbidden request — contact support |
  | 404 | Page not found |
  | 405 | Duplicate transaction |
  | 406 | Missing required information |
  | 407 | Invalid product code/token |
  | 408 | Result not found |
  | 409 | Invalid amount / transaction limit |
  | 410 | Insufficient money in wallet |
  | 434 | General operator-side error, transaction failed |
  | 435 | General database error, transaction failed |
  | 5xx | Server-side error |

- **Real gotcha, not yet handled by this repo's code, flagged for a
  future task rather than fixed here (doc-only pass):** `201` is
  documented as **"Transaction Pending — reverify"**, i.e. a real,
  meaningful non-final state — but `201` is also a `2xx` **HTTP**
  status, so `providers/payscribe.js`'s current `!response.ok` check
  (which only inspects the HTTP status) would treat a `201` response
  as success and return it via `processPayment`'s normal success path
  without ever surfacing that the transaction is actually still
  pending. Whether `status_code: 201` shows up in the *body* alongside
  a `2xx` HTTP status, or Payscribe actually sends HTTP `201` itself,
  isn't confirmed by any example captured this pass — either way, this
  is a real distinction the current code doesn't check for and should,
  once this becomes an implementation task.

*Webhook signature scheme — CONFIRMED IN FULL, resolves Task 6's core
blocker*
- Three headers on every inbound webhook: **`X-Payscribe-Event-Id`**,
  **`X-Payscribe-Timestamp`** (Unix seconds), **`X-Payscribe-Signature`**
  in the literal format **`v1=<64-char lowercase hex>`**.
- Signed content is the concatenation **`${timestamp}.${eventId}.${rawBody}`**
  (period-joined, same three-part-concatenation shape as DodoPayments'
  Standard Webhooks scheme above, but with different field order/count
  and a different algorithm) — **`HMAC-SHA256`** of that string, hex
  digest (not base64 — a real difference from DodoPayments), compared
  against the signature with a **constant-time comparison**
  (`hash_equals` in PHP, `crypto.timingSafeEqual` in Node.js,
  `hmac.compare_digest` in Python — all three official examples use
  the correct constant-time function, not `===`/`==`).
- **Replay protection is mandatory and explicit**: reject if
  `abs(now - timestamp) > 300` seconds, confirmed identically across
  all three language examples *and* independently restated in the
  "Common Mistakes to Avoid" page (mistake #3).
- The raw, unparsed request body must be used for signature
  verification — Payscribe's own docs call out re-serializing parsed
  JSON before verifying as mistake #1 (key-order changes would alter
  the bytes and break the signature), same caution DodoPayments/
  Standard Webhooks implementations need and this repo's existing
  Korapay/Paystack/JuicyWay handlers already get right.
- Idempotency: store `X-Payscribe-Event-Id` with a uniqueness
  constraint and skip duplicates (mistake #4) — directly relevant to
  Task 0/c-1's in-memory `provider:event:reference` dedupe store,
  which this repo would extend to cover Payscribe the same way as the
  other four providers, keyed on this header.
- **Must always return a `2xx` quickly** even if downstream processing
  is async (mistake #5) — Payscribe explicitly retries on any
  non-`2xx` response, same "ack fast, process after" requirement this
  repo's other webhook handlers already follow.
- **Confirmed real-name webhook events, from the sidebar nav (page
  titles, not bodies — see completeness caveat above):**
  `bills.payment.success` / `bills.payment.failed` (Bills Payment
  Webhook), `payout.created` / `payout.failed` (Payout Webhooks),
  `customer.created` / `customers.update` (Customers Webhook),
  **`accounts.payments.status`** (Collection Webhook, under NGN
  Virtual Accounts — **this is the one event this repo's existing
  `processPayment` flow actually needs**, since it creates a dynamic
  virtual account and would need to know when it's paid), plus
  `payment_link.paid`, `invoice.sent`/`invoice.paid`/
  `invoice.partially_paid`/`invoice.overdue`, `card.auth.refund`/
  `card.auth.verified`/`card.status.changed`,
  `savings.contribution.success`/`savings.contribution.failed`/
  `savings.withdrawal.success`/`savings.withdrawal.failed`.
- **Real, documented inconsistency flagged rather than smoothed
  over:** the PHP/Node/Python signature-verification *code samples'*
  own `switch`/`match` statements branch on `bills.created`, `cards.*`
  (a wildcard), `invoice.paid`, `payment_link.paid`, and one
  `savings.*` variant — **none of which exactly match** the sidebar's
  actual named webhook-reference pages above (`bills.payment.success`/
  `.failed`, not `bills.created`; specific `card.auth.*`/
  `card.status.changed` names, not a `cards.*` wildcard). These code
  samples read as **illustrative**, not an exhaustive/precise event
  list — a future implementation should switch on the sidebar's actual
  named events (especially `accounts.payments.status`), not copy the
  signature-verification examples' illustrative case labels verbatim.
- **Not confirmed by this pass:** the actual field-level payload shape
  of `accounts.payments.status` (or any other named event) — the
  webhook-reference pages exist in the nav but their bodies weren't
  captured in this snapshot. `routes.js`'s Payscribe webhook stub can
  be upgraded to verify signatures today using the scheme above, but
  mapping the *payload* into Task 0/b-3's normalized envelope still
  needs that specific page opened first.

*`processPayment`'s endpoint and `verifyTransaction`'s current
behavior — two real open items, neither confirmed nor contradicted by
this pass*
- `providers/payscribe.js#processPayment` calls
  `POST {baseUrl}/collections/virtual-accounts/create`. The sidebar's
  closest-named match is **"Create Dynamic (Temporary) Virtual
  Account"** under Collections → NGN Virtual Accounts — plausible by
  name and consistent with the request payload already being built
  (`account_type: 'dynamic'`, an `order`/`customer` shape), but that
  page's own body was **not captured** in this snapshot, so the exact
  literal path and required fields are **not confirmed against a
  primary source by this pass** — this audit makes the existing
  endpoint call *more plausible*, not *confirmed correct*.
- `verifyTransaction(reference)` currently just throws `'Payscribe
  verification requires Webhook or Bank Session ID. Please check
  Webhooks.'` The sidebar lists a real, named **"Verify Payment"**
  endpoint (POST, under Collections → NGN Virtual Accounts) whose body
  was likewise not captured — so this pass can **neither confirm nor
  correct** whether that thrown message is accurate (i.e., whether
  "Verify Payment" actually needs something this repo doesn't have,
  like a bank session ID) or stale (i.e., whether it just needs the
  `ref`/`trans_id` this repo already generates, the same shape as
  every other provider's `verifyTransaction`). Both of these need that
  specific page opened before either is touched — not fixed here, per
  the Discovery Convention's doc-only-pass rule, and not guessed at
  either way in the meantime.

*Currency — CONFIRMED NGN in every captured example, consistent with
this repo's existing omission from `CONFIRMED_PROVIDER_CURRENCIES`*
- Every request/response body captured this pass (Cable TV, Data
  Vending) is implicitly NGN — Cable TV's response includes an
  explicit `"currency": "NGN"` field; Data Vending's examples carry no
  currency field at all but are unambiguously Naira amounts. No
  captured source documents a currency *parameter* anywhere. **Not
  added to `CONFIRMED_PROVIDER_CURRENCIES` this pass** — same posture
  as Remita above: absence of a documented parameter isn't the same as
  a confirmed single-currency restriction, and the existing
  `getAmountFormat` guard (throw rather than guess) stays correct
  until a broader page (or the NGN Virtual Accounts endpoints
  specifically, which do accept a `currency` field per the existing
  code's payload) is captured and checked.

**Xixapay — FULL API discovery pass, audited 2026-09-06 (new —
resolves a-7, both the existence question and the discovery pass
itself; `providers/xixapay.js` does not exist yet, doc-research
only).** Per the Discovery Convention, a-7's first job was simply
confirming Xixapay is a real, documented provider — **it is**:
`xixapay.com` is a live Nigerian payments platform ("enabling Nigerian
businesses to receive payments"), with a real, structured GitBook docs
site at `documentation.xixapay.com`, its own `llms.txt` index, and
every page below fetched directly from that site this session. Scope
covered: Authentication, Error Codes, Customer (KYC), Virtual Account
(create/update), ID Verification, Payout, both Webhook pages (Virtual/
Dynamic Account, Card). **Explicitly not detailed below, confirmed to
exist by nav/name only:** the Card USD/NGN family beyond `Create a
Card` (Update Card Status, Withdraw from Card, Fund Card, Refresh Card
Details) — this platform's current no-DB payin/payout/webhook-relay
scope per Task 0 doesn't need card issuing yet; if a future task needs
it, that's its own follow-up read of already-known-to-exist pages, not
a fresh discovery pass.

*Environment & authentication — CONFIRMED, and structurally different
from every provider this repo already integrates: THREE credentials
required at once, not one or two*
- Single base URL: **`https://api.xixapay.com`** (no separate sandbox
  host documented anywhere in this pass — contrast with
  Paystack/Korapay/DodoPayments, which all have a distinct test-mode
  host or a test/live key pair on one host; Xixapay's docs never
  mention a sandbox/test environment at all, a real gap flagged for
  the product owner rather than assumed to not exist).
- Auth is confirmed, identically, on **every single endpoint page**
  fetched this pass (Customer, Virtual Account, ID Verification,
  Payout) as **three simultaneous credentials**: an `Authorization:
  Bearer {secret key}` header, a **separate** `api-key: {API key}`
  header, **and** a `businessId` field inside the request **body**
  itself. This is a real structural difference from every other
  provider in this file — Paystack/Korapay/JuicyWay/DodoPayments/
  Payscribe all authenticate with a single Bearer header; Xixapay
  layers three distinct identifiers (secret-in-header, key-in-header,
  business-id-in-body) on top of each other. This repo's existing
  `getProviderKey(provider, type)` pattern (`public`/`secret` per
  provider) doesn't currently have a slot for a third credential type
  — implementation would need either a new key type or to treat
  `api-key` as a second `secret`-class value, plus threading a stored
  `businessId` into every request body, a real per-request shape
  difference from how `processPayment(data)` builds a payload for any
  existing provider.
- **A word-for-word typo confirmed to repeat across every single
  request-header table in this doc site**, not a one-off: `Authorization`
  is documented as `Beaerer {Secrete_Key}` (sic, twice misspelled) on
  every page that shows it (Authentication Guide, Create/Update
  Customer, Create/Update Virtual Account, ID Verification, Payout).
  Flagged only because a consistent typo across an entire doc site
  (rather than one page) is itself a signal about how carefully this
  provider's docs are maintained — treat every other value on these
  pages with the same "verify against a real sandbox call before
  trusting the literal text" caution this file already applies to
  JuicyWay's docs.

*Response envelope — CONFIRMED INCONSISTENT across endpoints, a real
finding, not a copy-paste artifact of this audit*
- Customer creation/update responses use **`"status": true`** (a
  boolean, matching Payscribe/Paystack/Korapay's convention).
- Virtual Account creation, ID Verification, and every Payout response
  instead use **`"status": "success"`** (or `"failed"`) — **a string,
  not a boolean** — confirmed directly across three independent
  endpoint pages, not a single stray example. A future
  `providers/xixapay.js` cannot check `responseData.status` the same
  way for every Xixapay call the way this repo's existing providers
  do for theirs; the check needs to branch by endpoint family (boolean
  vs. string-literal `"success"`), or normalize both shapes at the
  point of use.
- **Update Customer's own documented success response is a
  confirmed copy-paste artifact from Create Customer**: `{ "status":
  true, "message": "Customer created successfully", "customer": {...}
  }` — an *update* endpoint returning the literal message "Customer
  **created** successfully." Not corrected here; flagged so a future
  implementer doesn't treat that exact message string as meaningful
  for branching logic.
- **The Create Virtual Account response example is malformed JSON in
  Xixapay's own docs** — a missing comma after `"accountTye": "static"`
  (also itself a misspelling of "accountType") immediately before
  `"Reserved_Account_Id"`, and a trailing comma after the `bankAccounts`
  array's closing brace. Confirmed by direct inspection of the page's
  own example block, not a fetch/rendering artifact — the shape (a
  `bankAccounts` array of `{ bankCode, accountNumber, accountName,
  bankName, accountTye }`, plus top-level `customer`/`business`
  objects) is still usable as a guide, just not literally copy-pasteable
  as valid JSON.

*Error codes — CONFIRMED, a real documented taxonomy, closer in shape
to DodoPayments' flat `{code,message}` than Payscribe's numbered table*
- Standard HTTP statuses (400/401/403/404/422/429/500/502/503), each
  with **stable machine-readable snake_case codes**: `bad_request`,
  `unauthorized`, `forbidden`, `not_found`, `unprocessable_entity`,
  `too_many_requests`, `internal_server_error`, `bad_gateway`,
  `service_unavailable` — confirmed at the Error Codes &
  Troubleshooting page, with example causes and suggested fixes for
  each. No rate-limit number is stated anywhere in this pass (only
  that a `429` exists) — contrast with Payscribe's documented 60/sec
  or DodoPayments' tiered table; flagged as unknown, not assumed
  unlimited, same posture as Remita's rate-limit gap above.

*Virtual Account creation — CONFIRMED, the closest analog to this
repo's `processPayment`, and a real design difference worth flagging*
- `POST /api/v1/createVirtualAccount`. Two mutually exclusive request
  shapes depending on whether an existing KYC'd `customer_id` is
  supplied (leaner body) or raw customer data is sent instead (fuller
  body, `id_type`/`id_number` required only when `accountType:
  "static"`, optional/omittable for `"dynamic"`) — confirmed directly,
  not inferred. `accountType` is either `"static"` (permanent,
  reusable, requires KYC/ID fields) or `"dynamic"` (temporary,
  requires `amount`) — the same static/dynamic distinction this repo's
  Payscribe integration already models (`account_type: 'dynamic'` in
  `providers/payscribe.js`), so the concept maps cleanly, but the field
  names and required-field logic differ enough (`bankCode` as an
  *array*, letting one call provision virtual accounts across multiple
  partner banks simultaneously — a real capability none of this
  repo's other four providers expose) that this is not a drop-in
  reuse of Payscribe's payload shape.
- **Partner-bank codes are a closed, confirmed list**, not a free-text
  field: `20867` (Palmpay), `20987` (Kolomoni MFB), `29007` (Safehaven,
  static+dynamic), `100004` (Opay, dynamic only) — a real constraint
  a future `providers/xixapay.js` would need to validate against
  before calling, since Opay's code is documented as dynamic-account-only.
- **No currency field appears anywhere in Virtual Account, Payout, or
  ID Verification requests/responses** — Payout's own docs explicitly
  state "All amounts are in NGN," and amounts throughout (Virtual
  Account's `1500`/`5000`, Payout's `25000`) are bare numbers in what
  are unambiguously Naira base units, not subunits. **Confirmed
  NGN-only for the payin/payout/collections surface this repo actually
  needs** — a stronger, more explicit finding than Payscribe's or
  Remita's "absence of a currency parameter" hedge above, since
  Xixapay's own payout docs state the NGN restriction directly rather
  than leaving it implicit. Card issuing is the one confirmed exception
  (Create a Card's response includes an explicit `"currency": "NGN"` or
  `"USD"` field, and `country: "US"` support with its own minimum
  amount) — but that's a separate resource family, out of this pass's
  detailed scope per the exclusion noted above, and shouldn't be read
  as multi-currency support for the Virtual Account/Payout surface this
  repo would actually integrate against.

*Webhook signature scheme — CONFIRMED, and a materially weaker,
simpler scheme than every other provider already in this file, a real
security-relevant gap flagged rather than glossed over*
- **A single raw header, literally named `xixapay`** (lowercase, no
  `X-` prefix, no versioning) carries the signature — confirmed
  identically on both the Virtual/Dynamic Account and Card webhook
  pages (verbatim-identical writeups on both pages, including the same
  code samples, suggesting one shared scheme across every Xixapay
  webhook resource rather than per-resource schemes).
- Signature is **`HMAC-SHA256(rawPayload, secretKey)`, hex digest,
  with no other inputs** — no timestamp, no event ID, no nonce
  concatenated into the signed content. This is a **real,
  confirmed difference from every other provider audited in this
  file**: Payscribe (`timestamp.eventId.rawBody`), DodoPayments
  (`id.timestamp.body`, Standard Webhooks), Korapay, and Paystack all
  bind a timestamp or a unique ID into what's signed specifically to
  block replay; **Xixapay's documented scheme has no replay protection
  at all** — a captured, valid webhook body+signature pair could be
  replayed indefinitely and would still verify. Not fixed here (doc-only
  pass); flagged as a real, provider-side gap for whichever session
  implements this, distinct from an implementation bug this repo could
  introduce on its own.
- **A confirmed, concrete bug in Xixapay's own official Node.js
  example, not a hypothetical**: the sample computes the signature
  over `JSON.stringify(req.body)` — i.e. **re-serializing the
  already-parsed body** — rather than the original raw bytes. This is
  the exact mistake Payscribe's own docs explicitly warn against
  (`Common Mistakes to Avoid`, mistake #1, in the Payscribe audit
  above): key-order or whitespace differences between the original
  raw request and the re-serialized JSON will silently break real
  signatures for some payloads, even though the PHP and Python
  examples on the same page correctly use the raw body
  (`file_get_contents('php://input')`, `request.body` in Django).
  Confirmed directly by reading the code sample, not inferred — a
  future `providers/xixapay.js#verifyWebhookSignature` should follow
  the PHP/Python pattern (verify against the untouched raw body) and
  explicitly not copy Xixapay's own Node.js sample as-is.
- **A second confirmed issue in the same Node.js sample**: it compares
  signatures with plain `===` rather than a constant-time function —
  the PHP sample correctly uses `hash_equals`, the Python sample
  correctly uses `hmac.compare_digest`, but the Node.js sample uses
  neither, unlike Payscribe's Node.js sample (which does use
  `crypto.timingSafeEqual`). A real, confirmed timing-attack surface
  in Xixapay's own reference implementation, specific to the
  language/example this repo would most likely copy from (this is a
  Node.js codebase).
- No idempotency guidance and no "always return 2xx quickly" guidance
  is given anywhere in either webhook page — contrast with Payscribe's
  explicit mistakes #4/#5 above. Not confirmed absent from Xixapay's
  actual retry behavior, just absent from the documentation same as
  Remita's rate-limit gap — don't assume no-retry, assume undocumented.
- **Payload field-level confirmed** for the Virtual/Dynamic Account
  webhook (the one this repo's `processPayment` flow would actually
  need): `notification_status`, `transaction_id`, `amount_paid`,
  `settlement_amount`, `settlement_fee`, `transaction_status`,
  nested `sender`/`receiver`/`customer` objects, `description`,
  `timestamp` (ISO 8601). **The example payload itself contains a
  literal JSON syntax error** in Xixapay's own docs (an unterminated
  string: `"email": "adexplug@gmail.com,` — missing closing quote) —
  confirmed by direct inspection, not a fetch artifact; the field list
  above is still trustworthy, the literal example block is not
  copy-pasteable as-is.

*Payout — CONFIRMED, single-currency (NGN), amount in base units*
- `POST /api/v1/transfer` (`businessId`, `amount`, `bank`,
  `accountNumber`, `narration`); a documented, separate
  `POST /api/verify/bank` pre-flight check (recommended, not
  enforced) and `GET /api/get/banks` for the supported-bank list.
  Response is `{ status: "success"|"failed", message, reference }` on
  the transfer call itself — the string-status inconsistency noted
  above applies here too. Xixapay's own docs state accounts must be
  exactly 10 digits and describe payouts as "instant," for whatever
  that's worth as an unverified marketing claim rather than an SLA.

**PaymentPoint — FULL API discovery pass, audited 2026-09-06
(supersedes the WEBHOOK-ONLY pass earlier this session; resolves a-8;
`providers/paymentpoint.js` does not exist yet, doc-research only, no
code written).** Source: the product owner supplied all six pages
directly (none found via this session's own web search) — saved
snapshots of PaymentPoint's Authentication, Errors, Webhook
Documentation, Create Virtual Account, Identity Verification, and
Liveness Check pages, all under `paymentpoint.gitbook.io/paymentpoint.co`.
The Webhook Documentation, Authentication, and Errors pages say "Last
updated 1 year ago" per their own footers; Create Virtual Account,
Identity Verification, and Liveness Check say "Last updated 3 months
ago" — treat the literal ages as unverified currency either way, same
posture already applied to every other provider's docs in this file.
**Scope confirmed complete, not partial:** these six pages match every
entry in PaymentPoint's own sidebar nav (Welcome / Let's get started /
Authentication / Errors / Virtual Account → Create Virtual Account /
Webhook Documentation / Verification → Identity Verification /
Liveness Check) — nothing in PaymentPoint's own nav was left unread
this pass.

*Environment & authentication — CONFIRMED, three simultaneous
credentials, same shape as Xixapay (a-7 above)*
- Single base URL: **`https://api.paymentpoint.co`** — no separate
  sandbox/test-mode host documented anywhere across any of the six
  pages (same gap already on record for Xixapay; contrast with
  Paystack/Korapay/DodoPayments, which all have a distinct test host
  or key pair).
- Confirmed identically on the Authentication, Create Virtual Account,
  Identity Verification, and Liveness Check pages: an `Authorization:
  Bearer {secret_key}` header, a **separate** `api-key` header, **and**
  a `businessId` field inside the request **body** — three
  credentials at once, the same structural shape already flagged for
  Xixapay, and the same misfit for this repo's existing
  `getProviderKey(provider, type)` `public`/`secret` pair.
- **A real finding, flagged rather than reproduced:** the
  Authentication page and the Create Virtual Account page's own
  request-header tables both display a **specific Bearer token and
  api-key value**, not an obviously-placeholder string like the `xxx`
  used elsewhere in PaymentPoint's docs. This audit deliberately does
  **not** copy that literal value into this repo (including this file)
  to avoid landing a live-looking secret in git history. Treat it as a
  possible real credential accidentally left in public documentation;
  the product owner should confirm and rotate it if so, not assume
  it's a dummy just because it appears in a docs page.
- **A second, smaller finding on the same Create Virtual Account
  page:** its request-body table's example `businessId` value is
  **identical to that same page's `api-key` example value** —
  almost certainly a copy-paste error, since the code sample
  immediately below that table uses a differently-shaped `businessId`
  (`3AB2B22345EF407`) that matches the format used consistently on the
  Identity Verification and Liveness Check pages' own code samples.
  Use the code-sample format, not the table's, if a real `businessId`
  isn't yet on hand.

*Error codes — CONFIRMED, a standard HTTP-status table, no
machine-readable error codes documented*
- 200/201 success; 400 bad request; 401 unauthorized; 403 forbidden;
  404 not found; 409 conflict (documented specifically as "using the
  same idempotent key for a previous request" — PaymentPoint has some
  idempotency-key concept, though no page in this pass documents its
  request-side mechanics, i.e. which header/field name carries it);
  429 rate limit (no numeric rate-limit figure given anywhere, same
  gap already on record for Xixapay); 500/502/503/504 server errors.
  **Unlike DodoPayments/Xixapay's snake_case error-code taxonomies,
  PaymentPoint's Errors page gives no stable machine-readable code per
  error** — only the HTTP status and a human-readable meaning, so
  error-branching logic in a future `providers/paymentpoint.js` has
  less to key off than for those other providers.

*Create Virtual Account — CONFIRMED, the closest analog to this repo's
`processPayment`/account-provisioning flow*
- `POST /api/v1/createVirtualAccount`. Request body: `email`, `name`,
  `phoneNumber`, `bankCode` (an **array** of partner bank codes — two
  confirmed values, `20946` = PalmPay, `20897` = OPay — consistent
  with the bank names already seen in the webhook payload's own
  `receiver.bank` field), `businessId`, and optional `idType`
  (`bvn`/`nin`) + `idNumber` (11 digits, required only when `idType`
  is set).
- Response: `{ status: "success", message, customer: {...}, business:
  {...}, bankAccounts: [{ bankCode, accountNumber, accountName,
  bankName, Reserved_Account_Id }], errors: [] }` — `status` is a
  **string**, matching the shape already seen in the Webhook
  Documentation's own `transaction_status` field, so at least these
  two PaymentPoint surfaces agree with each other (contrast with
  Xixapay, whose `status` field is confirmed inconsistent — boolean on
  some endpoints, string on others).
- `bankAccounts` is an **array** — a single virtual-account creation
  call can return more than one funded account (one per requested
  `bankCode`), which a future integration needs to handle as a list,
  not a single object.

*Identity Verification — CONFIRMED, paid NIN/BVN lookup, not required
for payment orchestration but real functionality if ever exposed*
- `POST /api/identity/verify`. Cost: ₦100/NIN lookup, ₦75/BVN lookup,
  billed from PaymentPoint's own wallet balance (`402 Insufficient
  Account Balance` if underfunded — a wallet-prefunding model, not
  per-request card billing); repeat lookups of the same number within
  30 days are documented as free (`cost_charged: 0` on a cached hit).
- Request: `id_type` (`nin`/`bvn`), `id_number` (11 digits),
  `businessId`. Response returns `personal_details` (name/DOB/gender/
  nationality, each field `null` if the registry doesn't hold it) and
  `biometric_data.photo` — a **base64-encoded JPEG with no `data:` URI
  prefix**. The doc's own text is explicit that this is biometric data
  and must not be written to logs or persisted beyond KYC-policy
  need — a real handling constraint for any future implementation, not
  optional guidance.
- Timeout guidance stated directly in the doc: set an HTTP timeout of
  **at least 90 seconds** (registry lookups documented as slow under
  load) — a client-side timeout followed by a naive retry can bill
  twice for one verification, since the doc also states a `404` (no
  record found) is not charged and should never be retried, while a
  `503` is transient and safe to retry with backoff.

*Liveness Check — CONFIRMED, paid selfie liveness check, ₦35/check,
one real gotcha in how to read the response*
- `POST /api/liveness/check`. Request: `image` (base64 selfie, max 2MB
  decoded, raw base64 only — a `data:image/jpeg;base64,` prefix causes
  a `422`; `selfie` accepted as an alias), `businessId`.
- **The one real gotcha, stated directly by the doc itself and worth
  repeating here because it's easy to get backwards:** a *failed*
  liveness check still returns **HTTP 200 with `"status":"success"`**
  — `status` only reflects "the API call itself succeeded," not
  whether the person passed. The actual verdict is the nested
  `data.is_live` boolean; a future integration that branches on the
  top-level `status` field instead of `is_live` would let every spoof
  attempt (printed photo, screen replay, mask) through silently. Same
  class of "don't trust the obvious top-level field" caution already
  on record for PaymentPoint's own webhook `notification_status` vs.
  `transaction_status` distinction above.
- Same wallet-billing model as Identity Verification (₦35 charged
  whether the check passes or fails, `402` if the wallet is
  underfunded) and the same biometric-data handling instruction (don't
  log the `image` field).

*Webhook payload — CONFIRMED, a flat JSON body, not envelope-wrapped
(carried over unchanged from this session's earlier webhook-only
pass)*
- Sent as a `POST` to the merchant's own configured URL on a completed
  payment. Confirmed top-level shape (from the doc's own literal
  example):
  ```
  {
    notification_status: "payment_successful",
    transaction_id: "xxx",
    amount_paid: 100,
    settlement_amount: 99.5,
    settlement_fee: 0.5,
    transaction_status: "success",
    sender: { name, account_number, bank },
    receiver: { name, account_number, bank },
    customer: { name, email, phone, customer_id },
    description: "...",
    timestamp: "2024-11-22T13:00:04.256092Z"  // ISO 8601
  }
  ```
- `amount_paid` is in **base currency units** (the example shows `100`
  for what reads as ₦100, not `10000` kobo) — contrast with this
  repo's `toSubUnit()`/`fromSubUnit()` helpers in `utils/helpers.js`,
  which currently only cover a 5-currency map tuned for Paystack's
  kobo-style subunits; a future `providers/paymentpoint.js` should
  **not** run PaymentPoint amounts through that conversion unverified
  — same caution already on record there for Korapay/JuicyWay/
  Payscribe.
- `sender.account_number` is explicitly **masked** in the documented
  example (`"****4290"`) — confirmed as the shape to expect, not an
  artifact of the one example; do not assume the full account number
  is recoverable from this field.
- `notification_status` and `transaction_status` are two distinct
  fields carrying overlapping-but-not-identical meaning in the one
  example given (`"payment_successful"` vs `"success"`) — the doc
  does **not** enumerate the full set of possible values for either
  field (no failed/pending example shown anywhere on this page), so
  neither field's full value space is confirmed. This is a real open
  item for Task 0/b-3's normalized-webhook mapping (`payment.succeeded`
  / `payment.failed` / `payment.pending`) — there is currently no
  confirmed documented value to map to `payment.failed` or
  `payment.pending` for this provider; get one from a real test
  webhook or ask the product owner before writing that mapping, don't
  invent a plausible-sounding string.
- `customer.phone` is documented as nullable (`null` in the example)
  — a future mapping must not assume it's always present.

*Signature verification — CONFIRMED, single HMAC-SHA256 over the raw
body, header name is provider-specific (carried over unchanged from
this session's earlier webhook-only pass)*
- Header: **`Paymentpoint-Signature`** (confirmed identically in all
  three of the doc's own PHP/Python/Node examples — PHP reads it as
  `$_SERVER['HTTP_PAYMENTPOINT_SIGNATURE']`, Python/Django as
  `request.headers.get('Paymentpoint-Signature')`, Node as
  `req.headers['paymentpoint-signature']`).
- Algorithm: `HMAC-SHA256(raw_request_body, secret_key)`, hex-encoded,
  compared against the header value. This is the **same algorithm and
  encoding** this repo already uses for Korapay
  (`providers/korapay.js`, `verifyWebhookSignature`) and structurally
  close to Paystack's (which uses SHA512 instead) — so, unlike Xixapay
  above, PaymentPoint's scheme is a reasonable fit for this repo's
  existing `crypto.createHmac(...).update(...).digest('hex')` pattern
  with no new shape needed.
- **A real gap, confirmed by absence, not by a negative statement in
  the doc:** like Xixapay, PaymentPoint's documented scheme has **no
  timestamp or nonce component** — a bare `HMAC(body, secret)` with no
  replay protection. This is a provider-side design limitation to note
  for this repo's dedupe-key mitigation (Task 0/c-1, already in place
  as `provider:event:reference` in-memory dedupe), not something a
  correct implementation of the documented scheme can add on its own.
- **A real security bug confirmed in PaymentPoint's own official Node
  sample, do not copy as-is:** the sample recomputes the signature over
  `JSON.stringify(req.body)` — i.e. the **parsed-then-re-serialized**
  body via `body-parser`'s `express.json()`-equivalent middleware —
  rather than the original raw bytes PaymentPoint actually signed.
  Key ordering, whitespace, and number formatting are not guaranteed
  to round-trip identically through parse-then-stringify, so this
  pattern can produce spurious signature mismatches (or, worse, false
  matches if an attacker can craft a payload that re-serializes to the
  same bytes as a legitimately-signed one). This repo's existing
  Korapay/Paystack verifiers already avoid this class of bug by
  hashing the raw body — the same discipline needs to carry over here;
  a future `providers/paymentpoint.js` must be wired to read the raw
  request body (e.g. via `express.raw()` on this specific route, same
  pattern already needed for Korapay/Paystack) rather than trusting
  `req.body` post-JSON-parse.
- **A second issue in the same official Node sample, also do not
  copy as-is:** the comparison is `if (calculatedSignature ===
  signature)` — a plain `===` string comparison, not a constant-time
  comparison. This repo's existing Korapay/Paystack verifiers both use
  `crypto.timingSafeEqual` (via a length-checked buffer comparison) —
  the same pattern should be used here rather than PaymentPoint's own
  sample, to avoid a timing side-channel. **This is the identical pair
  of bugs already flagged in Xixapay's own official Node sample above**
  — a second, independent confirmation that "copy the provider's own
  sample code verbatim" is not a safe default for this repo across
  providers generally, not a coincidence specific to either one.
- No mention anywhere on this page of a response-code contract for the
  webhook receiver (e.g. "respond 200 within N seconds or we retry") —
  unlike Korapay/Paystack, whose docs this repo already built against,
  PaymentPoint's webhook page is silent on retry/backoff behavior.
  Flagged as unknown, not assumed absent.

*Not covered by this pass — real open items, not yet resolved*
- No sandbox/test-mode behavior is documented anywhere across all six
  pages — unknown whether test-mode transactions/lookups use the same
  webhook shape, same header names, and same secret-key pairing as
  live mode.
- No confirmed value for a failed or pending `transaction_status` on
  the webhook — see above; blocks writing Task 0/b-3's
  PaymentPoint→normalized-event mapping until a real example (or the
  product owner) supplies one.
- No documented mechanics for the `409`/idempotent-key conflict
  response mentioned on the Errors page — which header or field
  actually carries an idempotency key is not stated on any of the six
  pages read this pass.
- Whether the live-looking credential flagged above is in fact real,
  and if so what it's scoped to, is unconfirmed — a question for the
  product owner, not something this audit can resolve from docs alone.

**Prestmit — FULL API discovery pass, audited 2026-09-06 (new —
resolves a-9, both the existence question and the discovery pass
itself; `providers/prestmit.js` does not exist yet, doc-research
only).** Per the Discovery Convention, a-9's first job was confirming
existence — **confirmed, with a name correction**: the real provider
is spelled **Prestmit** (not "Presmit" as Task 0 originally listed
it), a Lagos-based, Nigeria/Ghana-facing gift-card and crypto
off-ramp platform founded 2019, with real developer docs at
`documentation.prestmit.io` (GitBook-style, own `llms.txt` index, own
Postman workspace referenced from within the docs). Source: this
session's own web search (not product-owner-supplied, unlike a-8).
Pages fetched directly this pass: `llms.txt` (full index), API Keys &
Authentication, Managing Webhooks, IP Whitelisting, Payouts & Wallet
Details, and the Service Availability API-reference page (read for
its embedded OpenAPI spec, which turned out to matter — see below).
**Not fetched in per-page depth this pass, by deliberate scope
choice, not oversight:** the ~25 remaining gift-card buy/sell/lookup/
bank-account endpoints listed in `llms.txt` — see the scope-mismatch
finding immediately below for why this pass stopped at
authentication/webhooks/payouts/wallet rather than going endpoint-by-
endpoint through the full gift-card-trading surface; the full index
is preserved in `llms.txt` for whichever future session needs a
specific one of those endpoints.

*THE finding of this pass, stated first because it changes what "a-9
Prestmit" even means for this repo's orchestration layer: Prestmit is
not a bank/card payment processor*
- Every one of this repo's other nine providers (Korapay, Paystack,
  JuicyWay, Payscribe, DodoPayments, Flutterwave, Remita, Xixapay,
  PaymentPoint) exposes some form of "charge a customer" / "collect a
  payin" / "send a payout to a bank account" primitive that this
  repo's `processPayment()` orchestration is built around. **Prestmit
  has no such endpoint.** Its API's actual primitives are: create a
  gift-card **sell** trade (customer hands Prestmit a gift card,
  Prestmit pays the customer out), create a gift-card **buy** trade
  (customer pays Prestmit, Prestmit fulfills a gift-card code), and
  separately, wallet withdrawals/payouts of Prestmit's *own* partner
  balance to a bank account (not a customer-facing payout — see
  Payouts & Wallet Details below). None of these map cleanly onto
  "initiate a payment" the way Korapay/Paystack/PaymentPoint's
  `processPayment`-shaped endpoints do.
- **This is a real, unresolved scope question for the product owner,
  not an implementation detail this session can decide alone:** was
  Prestmit's inclusion in Task 0's provider list intentional (e.g.
  this platform means to offer gift-card liquidation as one more
  "payment method," alongside bank transfer/card/etc.), or was it a
  mix-up with a different, more conventional bank/card provider with a
  similar-sounding name? Either answer is plausible; guessing wrong
  wastes real implementation effort in either direction, so this stays
  open rather than assumed either way.

*Environment & authentication — CONFIRMED, but with a real, blocking
inconsistency: three different base-URL sources disagree with each
other*
- **Source 1 (API Keys & Authentication guide, prose):** Sandbox
  `https://dev-api.prestmit.io/partners/v1`, Live
  `https://api.prestmit.io/partners/v1`.
- **Source 2 (Service Availability reference page's embedded OpenAPI
  `servers` block):** `https://dev-api.prestmit.io` (host only, with
  `/partners/v1/...` then appended per-path) — consistent with Source
  1's sandbox host, at least.
- **Source 3 (that exact same OpenAPI document's own `info.description`
  prose, a few lines away from Source 2 in the identical file):**
  Staging `https://dev-2025.prestmit.io/api/partners/v1`, Production
  `https://prestmit.com/api/partners/v1` — **a completely different
  pair of hosts from both Source 1 and Source 2**, confirmed by direct
  inspection of the same fetched document, not a transcription error
  by this pass. This is the same class of "multiple primary sources
  disagree on the base URL" problem already on record for Remita
  (a-6) — get the current, actually-correct values from the product
  owner's own onboarding/sandbox-registration email (`sales@prestmit.com`
  per the Authentication guide) rather than picking one of the three
  and hoping.
- Auth itself, independent of which host is correct, is confirmed
  consistently: **HMAC-SHA256 over `API_KEY + ":" + JSON.stringify(body)`**,
  keyed with a separate `API_SECRET`, producing a 64-character hex
  digest sent as an `API-Hash` header, alongside a plain `API-KEY`
  header (not `Authorization: Bearer`, unlike every other provider in
  this file). **This is a genuinely different authentication shape
  from every one of this repo's other nine providers** — none of them
  sign the outbound *request* itself; they all authenticate with a
  static Bearer/API-key header and only apply HMAC signing to inbound
  *webhooks*. A future `providers/prestmit.js` would need new
  per-request signing logic this repo doesn't have anywhere yet, not
  a fit for the existing `getProviderKey()` pattern at all.
- One documented quirk worth carrying forward: the doc explicitly
  states any `attachments` field must be **stripped from the body
  before hashing** (attachments aren't included in the signed
  payload) — a real, easy-to-miss detail for any endpoint that accepts
  file uploads (e.g. the gift-card sell-trade creation endpoint, which
  takes `attachments[]`).
- The OpenAPI spec's own `securitySchemes` block only declares the
  plain `API-KEY` header (`type: apiKey, in: header, name: API-KEY`)
  and says nothing about the `API-Hash` HMAC requirement described in
  the prose Authentication guide — a real internal inconsistency
  between Prestmit's own machine-readable spec and its own written
  guide, on top of the base-URL disagreement above.
- **Confirmed, unlike Xixapay/PaymentPoint: a genuine, distinct
  sandbox environment does exist** (Source 1's `dev-api.prestmit.io`),
  though which of the three base-URL sources is authoritative for it
  remains the open item above.

*Webhooks — CONFIRMED, structurally the closest fit to this repo's
existing webhook-forwarding pattern, but event set is entirely
gift-card/wallet shaped, not payment shaped*
- Registered per-API-key via Prestmit's own console (one webhook URL
  per API key, HTTPS only), not passed as a request parameter.
- **Confirmed event set**, all gift-card-trade or wallet-withdrawal
  shaped, **none of them a generic "payment received" event**:
  `giftcard-trade.sell.approved`, `giftcard-trade.sell.rejected`,
  `giftcard-trade.buy.approved`, `giftcard-trade.buy.rejected`,
  `withdrawal-request.approved`, `withdrawal-request.rejected`,
  `withdrawal-request.cedis.approved`, `withdrawal-request.cedis.rejected`.
  Payload shape: `{ data: {...event-specific fields...}, event:
  "withdrawal-request.rejected", accountID: 58 }` — an `event`-typed
  envelope, closer in spirit to Task 0/b-3's own normalized-event
  design goal than most of this repo's other providers' raw payloads,
  though the event *names* here don't map onto b-3's fixed
  `payment.*`/`payout.*`/`refund.*` set at all — another consequence
  of the scope-mismatch finding above.
- Signature: **`x-prestmit-signature`** header, **HMAC-SHA256 over the
  raw body, base64-encoded** — the encoding is a real, confirmed
  difference from every other provider in this file (Korapay/Paystack/
  PaymentPoint all hex-encode; Prestmit base64-encodes), so a shared
  HMAC-verification helper covering all of this repo's providers would
  need an encoding parameter, not just a shared algorithm.
- **A real bug confirmed in Prestmit's own official webhook-
  verification sample — do not copy as-is, and note this is now the
  third provider in this file with the identical class of bug:** the
  sample's `verifyWebhookSignature(payload, signature, secret)`
  computes `crypto.createHmac("sha256", secret).update(JSON.stringify(payload))` —
  hashing the **re-serialized, already-parsed** payload object rather
  than the original raw request bytes. The doc's own later "Checklist
  for Secure Verification" section directly contradicts this same
  page's own code sample — it explicitly says "Always verify the raw
  body, not the parsed one" and "capture the raw request body before
  it's parsed" — so Prestmit's own documentation disagrees with
  itself, not just with this repo's standards. This is the identical
  bug class already flagged in Xixapay's (a-7) and PaymentPoint's
  (a-8) own official Node samples — a third independent confirmation
  that copying a provider's own sample code verbatim is not a safe
  default for this repo, across three unrelated providers now.
- The doc's own second sample (a standalone `testWithdrawalWebhook.js`
  test script) gets this right — it hashes `JSON.stringify(webhookData)`
  built locally before sending, which is consistent because in that
  script the sender and hasher share the exact same object; it isn't a
  counterexample to the bug above, it just doesn't demonstrate the
  parse-then-rehash failure mode since it never round-trips through a
  server's body parser.

*Payouts & Wallet — CONFIRMED, but this is Prestmit's own partner-
balance withdrawal, not a customer-facing payout primitive*
- `POST /wallet/fiat/create-withdrawal`, `GET
  /wallet/fiat/withdrawal-history`, `GET
  /wallet/fiat/withdrawal-history/{id}/receipt`, `GET
  /wallet/fiat/details` — all operate on **the partner's own prefunded
  wallet balance** (`NAIRA` or `CEDIS`), requiring a saved bank
  account, an account PIN, and conditionally a 2FA code. This is
  structurally similar to PaymentPoint/Xixapay's own wallet-billing
  model for verification lookups (a-8), not a payout-on-behalf-of-a-
  third-party primitive like Korapay/Paystack's `/payout`.
- **A real, confirmed field-naming inconsistency on this exact page:**
  the request-parameter table documents camelCase field names
  (`bankAccountId`, `currentAccountPIN`, `2fa_code`), but the page's
  own example request body directly below that table uses snake_case
  instead (`bank_account_id`, `current_account_pin`, `two_fa_code`) —
  two different naming conventions for what's presented as the same
  request, on the same page. Don't trust either version literally
  without a real sandbox call to confirm which one the API actually
  accepts.
- `amount` here is documented as **"in minor units"** — the opposite
  convention from PaymentPoint's webhook `amount_paid` (a-8, base
  units) — a real, provider-specific unit difference to get right if
  this repo's `toSubUnit()`/`fromSubUnit()` helpers are ever extended
  to cover Prestmit, not something to assume matches any other
  provider already in this file.
- Response's top-level `success` field is a **boolean** (`true`), not
  the string `"success"` seen in PaymentPoint's (a-8) and the OpenAPI
  General-endpoints examples elsewhere in Prestmit's own docs — yet
  another example, within this same provider's own documentation, of
  the boolean-vs-string `status`/`success` inconsistency already on
  record as a cross-provider pattern for Xixapay (a-7).

*Rate limiting — CONFIRMED to exist, numeric value unconfirmed
(example-only)*
- The Service Availability reference page's OpenAPI response schema
  includes example `X-RateLimit-Limit: 100` / `X-RateLimit-Remaining: 99`
  headers — a real rate-limiting mechanism exists, but the `100` is
  shown only as a schema example value, not stated anywhere in prose
  as the actual, current, enforced limit. Treat as "a limit exists,
  exact number unconfirmed," the same posture already applied to
  Xixapay's undocumented rate limit.

*IP Whitelisting — CONFIRMED, an optional security feature none of
this repo's other nine providers document*
- Per-API-key IP allowlisting, configured in Prestmit's own console,
  optional but recommended for production. Requests from a non-
  whitelisted IP are rejected (exact HTTP status not stated on this
  page). Worth flagging for the product owner's own deployment
  posture regardless of the scope-mismatch question above — this
  repo's outbound requests would originate from wherever it's
  deployed (Render, per `render.yaml`), a stable enough origin to
  whitelist if Prestmit integration proceeds.

*Not covered by this pass — real open items, not yet resolved*
- **The scope-mismatch question above is the primary blocker** — no
  further Prestmit implementation work should proceed until the
  product owner confirms whether gift-card trading was actually meant
  to be part of this platform's provider list.
- The three-way base-URL conflict is not resolved — needs the product
  owner's own onboarding materials, not a guess between the three
  documented options.
- No sandbox-vs-live behavioral differences are documented (e.g.
  whether test-mode trades/withdrawals fire the same webhook events).
- The ~25 gift-card buy/sell/lookup/bank-account endpoints listed in
  `llms.txt` were not individually audited this pass — deliberate
  scope choice, not an oversight; a future session doing gift-card-
  specific implementation work should read those pages directly rather
  than assume this entry covers them.
- Whether IP-whitelisting rejection returns 401, 403, or something
  else is not stated on the IP Whitelisting page itself.

---

**Flutterwave — FULL API discovery pass, audited 2026-09-06 (new —
resolves a-5; `providers/flutterwave.js` does not exist yet, doc-
research only).** Per the Discovery Convention, every page below was
fetched directly from `developer.flutterwave.com` this session
(official docs, confirmed live, with its own `llms.txt` index and
`.md`-suffix markdown mirrors of every page). Scope covered:
Authentication, Environments, Supported Request Headers, Encryption,
Errors, Webhooks, and the OpenAPI-defined reference pages for `Create
a charge`, `Initiate an Orchestrator charge`, and `Initiate an
Orchestrator transfer`. **The single biggest finding of this pass, and
the one that gates everything else: Flutterwave currently publishes
*two structurally incompatible API generations at once* (v3 and v4),
the docs portal defaults to v4, and this repo's other nine providers
all use v3-shaped single-static-secret-key auth — this is a genuine
version-choice open item, not a detail to pick silently, flagged in
its own subsection below rather than buried.**

*Version conflict — CONFIRMED, blocking, and structurally different
from every currency/base-URL conflict already recorded for other
providers in this file*
- **v3** (confirmed via `developer.flutterwave.com/v3.0/docs/
  authentication` and the trufflehog/keyhog secret-scanner detector
  entries for this provider, not this session's primary v4 pass):
  single static secret key sent as `Authorization: Bearer
  {secret_key}`, no token exchange, no expiry — identical in shape to
  every other provider already integrated in this repo (Paystack,
  Korapay, JuicyWay, Payscribe). Base URL `https://api.flutterwave.com/
  v3/...`. Keys are prefixed and pattern-matchable (`FLWSECK_TEST-`/
  `FLWSECK_LIVE-` for secret, `FLWPUBK_TEST-`/`FLWPUBK_LIVE-` for
  public), per the keyhog detector spec fetched this pass. **v3's own
  webhook scheme was not re-verified this pass** (this session's
  webhook audit below is v4-only) — a future session choosing v3
  must redo the Webhooks page read for v3 specifically rather than
  assume it matches the v4 findings below.
- **v4** (this pass's primary target, confirmed directly against
  `developer.flutterwave.com/docs`, which now defaults to v4 with a
  version-toggle to v3 still visibly present): authentication is
  **OAuth 2.0 client-credentials**, not a static key — see full
  breakdown below. This is a **materially different integration
  shape**, not just a renamed field: v3's Bearer-secret-key pattern
  this repo's `getProviderKey(provider, type)` already handles for
  every existing provider has **no analog** in v4, which instead needs
  a token-fetch-and-refresh manager (an `access_token` that expires
  every 10 minutes) sitting in front of every actual API call.
- Flutterwave's own public messaging (a DEV.to v4-beta announcement
  fetched during the initial search phase of this pass, not re-fetched
  for this file directly) states **v3 has no announced deprecation
  date and continues to be fully supported** — this is not a
  "v3 is legacy, ignore it" situation the way, say, an old SDK major
  version might be. **Open item, blocking, for the product owner
  directly, not resolvable from docs alone:** whether to build
  `providers/flutterwave.js` against v3 (matches every other
  provider's integration shape, but is the visibly-secondary option on
  Flutterwave's own docs portal) or v4 (the actively-promoted default,
  but requires this repo's first token-refresh-manager component,
  a new category of moving part — and expiring credentials sitting in
  a service explicitly built to have **no database** per Task 0's own
  constraint, meaning the access token and its expiry would have to
  live in-process memory only, re-fetched on every cold start). Not
  picked here — recorded as the load-bearing open question a future
  implementation session must get an answer to before writing any
  code, the same posture Remita's base-URL conflict and DodoPayments'
  currency conflict already established for this file.

*v4 Authentication — CONFIRMED, a full OAuth2 client-credentials flow,
not a bearer-key scheme*
- `POST https://idp.flutterwave.com/realms/flutterwave/protocol/
  openid-connect/token`, form-urlencoded body (`client_id`,
  `client_secret`, `grant_type=client_credentials`) → JSON response
  containing `access_token`, `expires_in` (confirmed **600 seconds /
  10 minutes** in the worked example), `token_type: Bearer`, `scope`.
  The returned `access_token` is then sent as `Authorization: Bearer
  {access_token}` on every actual API call — **not** the
  `client_id`/`client_secret` pair themselves, which only ever go to
  the token endpoint.
- Flutterwave's own official code samples (Node.js, Python, PHP, all
  three fetched directly from the Authentication page) all implement a
  token-caching-and-refresh pattern client-side (check
  `expires_in`/elapsed time, refresh when under ~60 seconds remain) —
  confirming this isn't an edge case Flutterwave expects integrators to
  ignore; a bare "fetch token, use once" implementation would work but
  contradicts Flutterwave's own recommended pattern and would incur an
  extra round-trip on every single charge/transfer call.
- Two separate credential-retrieval token endpoints are named across
  the fetched pages for the *same* OAuth flow — `idp.flutterwave.com`
  (Authentication page, Environments page) and `keycloak.dev-
  flutterwave.com` (inside the PHP SDK sample's `TOKEN_URL` constant,
  same Authentication page) — **a real, confirmed inconsistency within
  a single page's own code samples**, not a copy artifact from a
  different provider. Flagged rather than resolved; a future
  implementation session should verify directly against a real
  sandbox credential pair which host actually accepts the token
  request before hardcoding either one.

*v4 Environments — CONFIRMED, and containing a direct, confirmed bug in
Flutterwave's own multi-environment code sample*
- Test/sandbox base URL: **`https://developersandbox-api.flutterwave.com`**.
  Production base URL: **`https://f4bexperience.flutterwave.com`**
  (note: given with a trailing slash in the prose, without one in the
  cURL example — inconsistent but not itself a functional issue since
  a trailing slash is fine to strip). Both stated explicitly, twice
  each, in the Environments page's own prose and cURL examples.
- **The page's own "Multi-Environment Integrations" JavaScript example
  has the two base URLs swapped relative to its own prose two sections
  above on the same page**: `FLW_BASE_URL = isProd ?
  'https://developersandbox-api.flutterwave.com' :
  'https://f4bexperience.flutterwave.com'` — i.e. the sample sets the
  **sandbox** host when `isProd` is true and the **production** host
  when `isProd` is false, the exact opposite of what the same page
  states directly above it. Confirmed by direct side-by-side reading
  of the same fetched page, not a rendering artifact. A future
  `providers/flutterwave.js` must not copy this example's env-select
  ternary as-is — the correct mapping is sandbox = `developersandbox-
  api...`, production = `f4bexperience...`, per the page's own
  explicit prose statements, not per its own inverted code sample.
- Test data is archived after **30 days** and becomes permanently
  inaccessible after that — a stated data-retention limit, relevant
  for any future testing/QA plan, not just a documentation curiosity.
- v3-vs-v4 key visibility is tied together in one dashboard: the
  Environments page states a v3 account shows `Public Key`/`Private
  Key`/`Encryption Key` fields and a same-account toggle
  ("Switch to v4 live API keys") reveals v4 credentials instead —
  confirming v3 and v4 are two faces of the *same* underlying merchant
  account/dashboard, not two separate signups, which matters for
  whichever version the product owner ultimately picks (no separate
  "create a v4 account" step needed beyond the existing merchant
  relationship, if one already exists from earlier Flutterwave use).

*v4 Encryption — CONFIRMED, a third, distinct secret type, specific to
card charges only, not reused for OAuth or webhooks*
- Card charges require **field-level AES-256-GCM encryption** of the
  card number, expiry month, expiry year, and CVV individually, using
  a separate **encryption key** (retrieved from the same dashboard
  API-settings page as the OAuth `client_id`/`client_secret`, but a
  distinct value, not reused) plus a **12-character single-use nonce**
  generated per request. Confirmed directly from the Encryption page's
  own worked example and three official code samples (Node.js Web
  Crypto `AES-GCM`, Java `javax.crypto` `AES/GCM/NoPadding`, Python
  `cryptography`'s `AESGCM`) — all three independently implement the
  same AES-256-GCM-with-nonce scheme, not just one language sample
  guessing at it.
- This means a v4 card charge needs **three separate secret values in
  play at once** — OAuth `client_id`+`client_secret` (to get an
  `access_token` for the request's own `Authorization` header),
  the field-encryption key (to encrypt the card object before it goes
  in the request body), and a per-request nonce (not secret, but must
  be exactly 12 characters and unique per encryption call) — a
  materially more complex credential shape than any other provider
  already in this file, including Xixapay's three-simultaneous-
  credentials finding above (Xixapay's three are all *sent* per
  request; Flutterwave's encryption key is used **client-side only**,
  before the request is built, and never appears in the request
  itself).
- Sending unencrypted or malformed-encrypted card fields returns a
  confirmed, specific `422` (`CLIENT_ENCRYPTION_ERROR`, code `11100`,
  "Unable to decrypt encrypted fields provided") rather than a generic
  validation error — worth wiring a specific message for, not lumping
  into a catch-all 4xx handler.
- Non-card payment methods (bank transfer, mobile money, USSD, wallet)
  confirmed to need **no field-level encryption at all** in this
  scheme — only the `card` object's four fields require it. A future
  `providers/flutterwave.js` only needs the AES-256-GCM path wired for
  the card charge case, not universally.

*v4 Charge creation — CONFIRMED, and the shape differs sharply
depending on which of two charge endpoints is used*
- **`POST /charges`** (the plain, OpenAPI-documented endpoint) requires
  a **pre-existing `customer_id` and `payment_method_id`** in its
  request body — i.e. a customer record and a payment-method record
  must each be created via their own separate endpoints *before* this
  call, a two-extra-round-trips-minimum flow. This does not match how
  this repo's existing providers work (Paystack/Korapay/JuicyWay/
  Payscribe all accept inline customer + raw payment details in a
  single call) and would be a poor fit for this repo's stated no-DB,
  single-call orchestration goal.
- **`POST /orchestration/direct-charges`** (the "Orchestrator" helper
  endpoint, confirmed via its own separate OpenAPI reference page) is
  the actual analog to every other provider's single-call charge —
  it accepts an inline `customer` object (email required; name/phone/
  address optional) and an inline `payment_method` object in the same
  request, with no pre-created IDs needed. **This is the endpoint a
  future `providers/flutterwave.js` should target for v4**, not the
  plain `/charges` endpoint — confirmed by directly comparing the two
  endpoints' own OpenAPI schemas (`charge_in` requires `customer_id`
  + `payment_method_id`; `direct_charge_in` requires `customer` +
  `payment_method` as inline objects instead).
- Both endpoints share one currency enum (confirmed identical list
  across both OpenAPI documents fetched this pass) that is **far wider
  than every other provider already in this file**: standard ISO 4217
  codes across every region Flutterwave serves, plus three
  **non-ISO stablecoin codes appended to the same enum** — `USDC`,
  `USDT`, `RLUSD` — a genuinely unusual design (stablecoins listed as
  ordinary currency values alongside NGN/USD/EUR, not as a separate
  payment-method type), confirmed directly from the schema, not
  inferred from marketing copy. `amount` on both endpoints is a
  **decimal, in major currency units** (`12.34`, not `1234`), a real,
  confirmed difference from Korapay/Paystack's kobo-style integer-
  subunit convention already established elsewhere in this file — a
  future `providers/flutterwave.js` must not reuse this repo's
  existing amount-unit-conversion helper for Korapay/Paystack as-is.
- `reference` is validated by regex on both endpoints (`^[a-zA-Z0-9\-
  ]+$`, 6–42 characters) — alphanumeric-plus-hyphen only, no other
  punctuation, a concrete constraint this repo's own reference
  generator (per Decision 1 above) would need to satisfy for
  Flutterwave specifically.
- **Card fields in the `direct_charge_in` schema are the encrypted
  ones** (`encrypted_card_number`/`encrypted_expiry_month`/
  `encrypted_expiry_year`/`encrypted_cvv`/`nonce`, required together)
  — confirming the Orchestrator endpoint is not an "encryption-
  optional" shortcut; the AES-256-GCM requirement above applies to it
  identically.
- A documented **timeout-and-requery pattern**, confirmed via the
  Errors page's own worked example: charge/order requests that don't
  get a definitive result within roughly 25–28 seconds return `HTTP
  201 Created` with `next_action.type: "requires_requery"` rather than
  an error — the integrator is expected to wait **20 seconds**, then
  poll the retrieve-charge endpoint, and repeat with a **40-second**
  wait if still `requires_requery`; transactions unresolved after
  about a minute transition to `failed`. This is a materially
  different in-flight-uncertainty pattern from every other provider's
  synchronous success/fail response already in this file, and the docs
  explicitly recommend adding an `X-Idempotency-Key` header specifically
  to make safe retries of this scenario possible.

*v4 Errors — CONFIRMED, a large, stable, machine-readable taxonomy,
closer in shape to Xixapay's snake_case codes than Payscribe's numbered
table, but far larger than either*
- Standard envelope: `{"status": "failed", "error": {"type", "code",
  "message", "validation_errors"?}}` on every 4xx/5xx, confirmed
  identically across the Errors page's own example and both charge-
  endpoint OpenAPI schemas' `400`/`401`/`403`/`409` response
  definitions (i.e. this isn't just prose describing an aspiration —
  the same shape is baked into the machine-readable OpenAPI spec
  itself).
- **~70 collections/inflow-specific error codes** and a further ~70
  **payout-specific** error codes, each with a stable numeric-ish
  `code` and a `SCREAMING_SNAKE_CASE` `error.type`, confirmed
  enumerated directly on the Errors page (not summarized from a
  shorter table) — meaningfully larger and more granular than any
  other provider's error taxonomy already recorded in this file.
  Notable ones relevant to a no-DB relay layer: `CHARGE_ALREADY_EXISTS`
  (`1100409`, reference reuse), `CARD_NOT_TOKENIZED` (`1104400`,
  raw card data sent where a token/nonce was expected — relevant given
  the encryption requirement above), `INVALID_CHARGE` (`1150400`,
  "Please use the /virtual-accounts resource" — meaning bank-transfer-
  style collections are a **separate resource family** from `/charges`
  entirely, not a `payment_method.type` value on the charge endpoints,
  a real structural fact confirmed by the error message's own wording,
  not inferred).
- **Rate limit confirmed as a flat 500 requests/minute**, a single
  documented number (not a "contact support" hedge the way Xixapay's
  rate limit was left undocumented above) — this repo's `/pay`/
  `/payout` routes calling out to Flutterwave should budget against
  this directly. The Errors page separately notes intermediary
  infrastructure (Cloudflare) may return **non-JSON** throttling pages
  under sustained abuse — a real parsing edge case (a future
  `providers/flutterwave.js`'s response-error-parsing code must not
  assume every non-2xx body is valid JSON).

*v4 Webhooks — CONFIRMED, and containing a direct, confirmed
contradiction between two sections of Flutterwave's own single
Webhooks page*
- Single raw header, **`flutterwave-signature`** (lowercase, no `X-`
  prefix) — structurally the same "one flat header, no versioning"
  shape as Xixapay's `xixapay` header above, not Payscribe/
  DodoPayments' composite-header schemes.
- The page's own **"Verifying Webhook Signatures" section** (the
  canonical description, presented first) states the signature is
  `HMAC-SHA256(rawBody, secretHash)`, **base64-encoded** — a real,
  confirmed difference from every other provider in this file, all of
  which use **hex** digests (Paystack, Korapay, Payscribe, Xixapay all
  confirmed hex above) — copying an existing hex-comparison helper
  as-is for Flutterwave would silently and permanently fail every
  signature check.
- **The same page's own later "Examples" section directly
  contradicts its own canonical description**: the Node.js/PHP/Python
  webhook-handler examples in that section all compare the incoming
  `flutterwave-signature` header **directly against the raw
  `secretHash` value itself** (`signature !== secretHash`) — i.e. they
  never compute an HMAC at all, contradicting the HMAC-SHA256-of-the-
  body scheme stated two sections above on the identical page.
  Confirmed by direct side-by-side reading of both sections in the
  same fetched document, not a version-mismatch between separately-
  fetched pages. A future `providers/flutterwave.js#verifyWebhookSignature`
  must implement the **HMAC-SHA256-then-base64** scheme from the
  "Verifying Webhook Signatures" section, not copy the "Examples"
  section's simpler-looking but contradictory direct-comparison code.
- **Neither version of the comparison code uses a constant-time
  compare** — the "Verifying Webhook Signatures" section's own sample
  uses plain `===`/`hash === signature`, and the "Examples" section's
  three language samples use plain `!==`/`==` comparisons too. This is
  the same confirmed timing-attack-surface finding already recorded
  for Xixapay's Node.js sample above, but here it's **every one of
  Flutterwave's own samples, in every language shown**, not just the
  Node.js one.
- Webhook timeout confirmed at **60 seconds**; failed/unreachable
  webhooks are retried **3 times at a 30-minute interval** (both
  numbers stated directly, not inferred) — a materially longer retry
  spacing than typical exponential-backoff schemes, worth noting for
  any future reconciliation/backfill logic built on top of this
  gateway.
- Payload envelope confirmed as `{data: {...}, type, id, timestamp}` —
  a `type` field (`charge.completed`, `transfer.disburse`,
  `transfer.reversal`, `order.authorization`, `refund.completed`, all
  five confirmed directly from the shared OpenAPI `webhooks` block
  attached to the charge-endpoint reference pages, not guessed from
  the prose page alone) rather than a boolean success flag the way
  several other providers in this file use — this repo's existing
  `webhookGateway.js` dispatch-by-provider logic would need a
  dispatch-by-`type`-field branch for Flutterwave specifically, not a
  dispatch-by-status-boolean branch.
- Flutterwave's own **Best Practices** subsection on the same page
  explicitly recommends the same three defensive patterns this file
  has already flagged as *missing* from other providers' own docs
  above (always re-verify via the API before giving value; don't rely
  solely on webhooks, poll as a backup; be idempotent, dedupe on
  unchanged status) — worth noting as a positive contrast, not just a
  list of gaps, since Payscribe/Xixapay's own docs were confirmed
  silent on idempotency/backup-polling guidance above and Flutterwave's
  are not.

*v4 Payouts (Transfer Orchestrator) — CONFIRMED, and the single most
currency-fragmented request shape of any provider in this file*
- **`POST /direct-transfers`** is the single-call analog to every
  other provider's payout call (confirmed via its own OpenAPI
  reference page, separate from the plain `/transfers` endpoint, which
  — like plain `/charges` — needs pre-created recipient/sender IDs
  first and is not the fit for this repo's no-DB single-call model,
  by direct analogy to the charges-vs-orchestrator-charges finding
  above, not independently re-confirmed line-by-line this pass).
- **The request body's required recipient/sender fields are entirely
  currency-dependent**, via a `oneOf`+`discriminator` schema keyed on
  `destination_currency` — confirmed directly from the OpenAPI
  document, not inferred: an NGN payout needs only
  `bank.account_number`+`bank.code` (matching the minimal shape this
  repo's other NGN-only providers already use); a EUR, GBP, USD, or
  ZAR payout instead **requires full recipient KYC** — first/last name,
  phone, email, and a complete postal address — **plus**, for EUR/GBP/
  USD specifically, an equally complete **sender** KYC block
  (name/national ID or date-of-birth/phone/email/address). This is a
  categorically different, and categorically larger, payload
  requirement than any payout shape already recorded in this file for
  any other provider — a future `providers/flutterwave.js`'s payout
  function cannot use one fixed request-body shape the way this repo's
  existing `processPayout` does for Korapay; it needs currency-
  conditional payload assembly from the start.
- `action` (`instant`/`deferred`/`scheduled`) plus an optional
  `disburse_option` (`date_time` + IANA `timezone` from a fixed,
  confirmed enum list) is a scheduling capability **not present in any
  other provider's payout call already documented in this file** —
  genuinely new surface area, not a renamed existing field.
- Payout error taxonomy (confirmed on the Errors page, ~70 codes) is
  its own numeric-code family, separate from the charge-side codes
  above, and includes several compliance/geofencing-specific codes
  worth flagging for a global orchestration layer specifically:
  `ZAMBIA_TRANSFER_UNAVAILABLE` (Zambia payouts restricted to Zambia-
  registered merchants only), `XAF_TRANSFER_ONLY` (XAF-involving
  transfers restricted to XAF-to-XAF only without separate approval),
  `IP_WHITELISTING_REQUIRED`/`NON_WHITELISTED_IP`/`BLACKLISTED_IP` (an
  IP-allowlisting gate on the transfer API specifically, the same
  category of constraint Prestmit's own audit above flagged, now
  confirmed present for Flutterwave's payout surface too — worth
  checking this repo's Render-hosted egress IP against Flutterwave's
  dashboard allowlist before payouts go live, same recommendation
  already made for Prestmit above).

*Not covered by this pass — real open items, not yet resolved*
- **The v3-vs-v4 version choice above is the primary blocker** — no
  further Flutterwave implementation work should proceed until the
  product owner picks one, for the reasons laid out in that subsection
  (integration-shape mismatch with every other provider vs.
  actively-promoted-but-heavier default).
- The `idp.flutterwave.com` vs `keycloak.dev-flutterwave.com` token-
  endpoint inconsistency is not resolved — needs a real sandbox
  credential pair tested against both hosts, not a guess.
- v3's webhook signature scheme, error taxonomy, and payout shape were
  **not** re-audited this pass (v4 was this session's target) — a
  future v3-track implementation session needs its own read of the
  v3-specific docs, not an assumption that the v4 findings above
  transfer over.
- Subscriptions, Settlements, Refunds, Chargebacks, and the full set of
  currency-specific payout instruction schemas beyond NGN/EUR/GBP/USD/
  ZAR (ETB, GHS, MWK, and the mobile-money-specific EGP/ETB/XAF
  variants all appeared in the OpenAPI schema fetched this pass but
  were not individually detailed above) are out of this pass's scope —
  this platform's current no-DB payin/payout/webhook-relay focus per
  Task 0 doesn't need them yet; a future session adding any of those
  should read those specific schema branches directly rather than
  assume this entry covers them.
- No sandbox-vs-live *behavioral* differences (e.g. whether test-mode
  webhooks fire identically to production ones) were confirmed either
  way this pass, beyond the stated 30-day test-data archival limit.

---

## Project owner decisions (recorded verbatim from the owner — resolves previously open questions; read before touching reference/idempotency or anything wallet-related)

### Decision 1 — Reference generation + ownership, and who calls this backend (resolves the storage question Task 12 deliberately left open, see "Known issues" below)
**Correction from the project owner to this decision (see chat, this
supersedes the first version of this note):** the client app does **not**
call this backend directly. The actual flow: the app generates the
`reference` client-side at the moment payment is initiated (not shown to
the user) and writes it to Supabase. From that point on, the **Supabase
Edge Function is this backend's caller** — it is the edge function, not
the app, that calls this backend's `POST /pay` (passing that same
reference through), and it's the edge function that receives the
provider's webhook, matches it against the reference already sitting in
the Supabase table, and writes the result back to that table. So in
production this backend is called by Supabase infrastructure, not
directly by the mobile/web client.
This still settles Task 12's open question the same way as before: this
backend does **not** need its own idempotency store (SQLite/JSON file,
etc.) — that responsibility lives on the Supabase side, which already has
a database. This backend's role stays what Task 12's in-scope half
already built: accept whatever `reference` it's given, validate its
format, and forward it as-is to the provider. The only thing that changed
from this decision's first draft is **who** that caller is (the Supabase
Edge Function, not the app) — not what this backend itself does with the
reference.

**Addendum, generalized for Task 41's multi-tenant gateway — product
owner confirmed directly, same principle broadened beyond mavins-web
specifically:** this backend gets **no database, ever, structurally**
— not just for mavins-web's idempotency, but as a permanent
architectural rule for every app that uses this backend as its
canonical payment gateway. Every such app already has its own
database; this backend's job stops at verifying Korapay's signature
once and forwarding the event to whichever app owns it (Task 41's
`webhookGateway.js`), and durable recording is each app's own
responsibility via its own edge function receiving that forward. See
Task 41's own entry for the full write-up — this addendum just
confirms it's the same "no DB here" principle as this decision above,
now stated as a standing rule rather than a per-task conclusion.

### Decision 2 — Wallet crediting logic (Supabase/Mavins-web side, not this backend — noted here for continuity)
Once the Supabase Edge Function confirms a webhook, Supabase computes the
wallet-balance update: the user pays the full campaign amount *plus* the
platform fee up front; on confirmed receipt, the platform fee is deducted
and only the **remainder** is what shows as the user's wallet balance.
Whether anything is shown in the wallet at all depends on how the user
got there:
- **First-time users pay directly for a campaign** — there is no "top
  up wallet, then spend from wallet" step for a new user. All new users
  pay directly.
- **Only returning users top up a wallet balance** ahead of spending it
  on a future campaign.
- So: if a **new** user pays directly and the webhook is confirmed, they
  do **not** see a wallet balance change at all — they see a success
  screen only ("your campaign is live"), never a wallet number. Wallet
  balance display is a returning-user-only concept.

### Decision 3 — Post-payment success UI (frontend concern, Mavins-web — noted here for continuity)
On confirmed payment, both the paying user and admin (viewing the same
campaign) see the same success treatment: a success screen stating the
campaign is live, plus an animated workflow/pipeline visualization showing
interconnections to the countries the user selected for the campaign,
radiating out from a central "hub" node. This is a shared user/admin view,
not two different screens.

**Where this belongs:** Decisions 2 and 3 are Supabase/Mavins-web
concerns, not B-Pay-backend concerns — this backend only initiates charges
with a provider, it doesn't own wallet balances or the post-payment UI.
Recorded here anyway per the project owner's request so no session
(in this repo or Mavins-web) re-asks or re-derives it. Per this file's own
"Cross-repo continuation" pattern (see below), whichever session next
touches Mavins-web should copy Decisions 2 and 3 into that repo's own
`handover.md` and open real implementation tasks there — this file isn't
the place to design that UI/wallet code, just to preserve the decision.

---

## Known issues already found (not yet fixed — each becomes its own task below)

- `render.yaml` runs `buildCommand: npm install && npm run build` and
  `startCommand: node dist/index.js`, but `package.json` has **no
  `build` script** and there is **no `dist/` directory or build step
  anywhere** in this repo (it's plain ESM `.js`, not compiled). As
  configured, a fresh Render deploy of this exact repo would fail at
  the build step. `tsconfig.json` exists but nothing in `package.json`
  invokes `tsc`, and the source is `.js` not `.ts` anyway, so it's
  unclear if the tsconfig is even meant to be used for a build, or is
  leftover scaffolding. **Investigate before assuming — this may
  already be handled by a different Render service config than what's
  in this repo's `render.yaml`, or `render.yaml` may be stale.**
- **No webhook receiver endpoint exists at all** — `routes.js` only has
  `POST /pay` and `GET /verify`. Every provider's real source of truth
  for "did the payment actually succeed" is a webhook, not client-side
  polling of `/verify`. This is a significant gap, not a small one —
  it's broken into multiple small tasks below rather than one big one.
- `ROUTING_RULES` in `routes.js` picks a provider from an abstract
  `action` string (`collect_payment` / `bank_transfer` / `payout` /
  `international`) — it has no awareness of currency, country, or
  which providers actually support which currency. A request for a
  currency none of the routing logic considered would silently go to
  whatever `action` maps to, regardless of whether that provider can
  actually process it.
- `toSubUnit()` / `fromSubUnit()` in `utils/helpers.js` hardcode a
  5-currency map (NGN, USD, GHS, KES, ZAR) with the same ×100 multiplier
  for all of them, and default anything unrecognized to ×100 too. This
  is only correct for Paystack. It is not applied to Korapay, JuicyWay,
  or Payscribe payloads at all currently (their `processPayment`
  methods pass `data.amount` straight through) — **confirmed correct
  for Korapay** as of Task 7 (base currency units, not subunits), but
  still not verified one way or the other for JuicyWay or Payscribe.
- ~~No request body validation on `POST /pay` beyond checking `amount`
  is truthy — no type check, no positivity check, no currency format
  check, no customer-object shape check.~~ **Resolved by Task 11** —
  see that task's note for what's actually validated now
  (amount/currency format/customer.email where the resolved provider
  requires it).
- No idempotency protection when the client doesn't supply their own
  `reference` — `generateReference()` mints a fresh one on every call
  in that case, so a client retry (e.g. a double-tap on mobile, with
  no client-side reference of its own) can still create two separate
  charges for what the user experienced as one action. **RESOLVED by
  the project owner — see "Project owner decisions" → Decision 1
  above:** the app generates the reference client-side and stores it in
  Supabase; a **Supabase Edge Function** — not the app directly — is
  this backend's actual caller, forwarding that same reference through
  when it calls `POST /pay`, and that same edge function is also where
  webhook receipt of record and reconciliation happen. This backend
  does not build its own idempotency store — it only validates and
  forwards whatever `reference` it's given (already done, Task 12). In
  practice this means this backend's real-world caller (the edge
  function) is now expected to always supply a client-originated
  reference rather than rely on this backend's fallback
  `generateReference()`, so the true no-reference-supplied double-charge
  case above should mostly stop occurring once the edge function is
  built to match Decision 1 — see the new Task 23 below.
- No rate limiting anywhere. **Still open** — Task 13's error-handling
  pass (below) deliberately did not touch this; it needs its own
  session.
- ~~Provider error messages are passed back to the client close to
  verbatim...~~ **Addressed by Task 13's error-handling-review half**:
  provider-authored messages (meant for the end user) are now
  explicitly flagged safe-to-pass-through via `providerError()`
  (`utils/helpers.js` + all four provider files); everything else
  (network failures, missing API keys/base URLs) gets a generic
  client-facing message while the real detail stays in the server log.
  See Task 13's own note for the full write-up.

---

## Current focus: Korapay only (as of 2026-08-27)

**Project owner direction: narrow scope to Korapay for now.** We are
still waiting on API keys from Paystack, JuicyWay, and Payscribe, so
there is no way to test or verify anything beyond what's already
committed for those three providers. Until those keys arrive:

- **Do** keep working the queue for any task that is Korapay-specific
  (currently: Task 7).
- **Don't** start Task 6 (Payscribe — already blocked on docs anyway),
  Task 8 (Paystack endpoint verification), or the Paystack/JuicyWay/
  Payscribe portions of any multi-provider task (9, 10, 11, 12, 13) —
  leave their checkboxes unchecked and skip over them. **Exception
  exercised 2026-09-06:** Task 8's own note already allowed the
  doc-research half to proceed without keys ("Doc research alone
  doesn't need a key"); that half is now done (see Task 8 and the
  "Confirmed research findings" section). The end-to-end,
  key-required half of Task 8 is still on hold, unchanged by this.
  Two new Paystack-specific code tasks came out of that doc pass
  (Task 8c, Task 8d) — both are still genuinely blocked by this
  narrowing like any other Paystack code task, except noted otherwise
  in Task 8d's own entry (a static list change needs no live key).
- Multi-provider tasks (9, 10, 11, 12, 13) that don't strictly require
  the other three providers' credentials may still get a **Korapay-only
  partial pass** if a session finds a clean way to scope the work that
  way (e.g. Task 9's `getAmountFormat(provider, currency)` shape could
  be designed generically and filled in for Korapay alone, leaving the
  other three providers' entries as explicit TODOs rather than guesses)
  — but don't force it if the task doesn't split cleanly; when in
  doubt, skip and leave a one-line note here instead of half-finishing
  a task that needs all four providers to make sense.
- This is a temporary narrowing, not a permanent re-scoping of the
  project — once API keys for the other providers arrive, remove this
  section (or mark it resolved) and resume the full queue in order,
  starting from whatever's still unchecked.

---

## Task queue

Do the first unchecked task, in order, **except while the "Current
focus: Korapay only" section above is active — then do the first
unchecked Korapay-specific task instead**, skipping non-Korapay tasks
that need credentials we don't have yet. Do not skip ahead for any
other reason unless a task explicitly says its prerequisite isn't ready
(e.g. Payscribe waiting on docs).

### Task 1 — Fix or diagnose the Render build mismatch [x]
`render.yaml` builds with `npm run build` and starts `node
dist/index.js`; `package.json` has no `build` script and there's no
`dist/`. Figure out what's actually true: is this repo deployed via
`render.yaml` at all, or does Render's dashboard have a different,
manually-configured start command that overrides the file? If you
can't determine that from the repo alone, the safest fix is to make
`render.yaml` match reality — `buildCommand: npm install`,
`startCommand: node index.js` — since there's no compile step needed
for plain ESM JS. Don't add a real TypeScript build step as a side
effect of this task; that's out of scope here even though
`tsconfig.json` exists (flag it as a separate potential future task
instead, don't act on it).

**What was found / what changed:** Could not reach Render's dashboard
directly (no credentials, and the live URL isn't discoverable via
web_search to fetch its actual response), so went with the task's
own specified fallback rather than guess. Confirmed from the repo
itself: `package.json` has only `"start": "node index.js"` — no
`build` script at all — and there is no `dist/` directory anywhere in
the tree (`ls dist` → no such file). Also confirmed there are **no
TypeScript source files** (`find . -name "*.ts"` → empty) —
`tsconfig.json` is unused boilerplate (looks like an untouched
`tsc --init` output, still has its default commented-out options).
Since the app that's live at `b-pay-backend.onrender.com` (per the
project's own notes) is presumably actually running *something*, the
most likely real-world explanation is Render's dashboard has a
manually-configured start command that already overrides
`render.yaml` (Render lets dashboard settings win over the file when
both exist) — but that's an inference, not confirmed directly, since
dashboard access wasn't available this session. Fixed `render.yaml`
to match what the repo actually is either way: `buildCommand: npm
install`, `startCommand: node index.js`. This makes the file correct
and safe to use if/when the project ever does trigger a redeploy from
it (e.g. via "Clear build cache & deploy" or a fresh Render service),
without touching anything on the live dashboard. Did **not** add a
real TypeScript build step — flagging that as a legitimate future
task if the project ever wants `tsconfig.json` to do something real,
but out of scope here per the task's own instruction.
Verified with `node --check index.js` and `node --check routes.js`
(both pass — this task only touched `render.yaml`, a config file with
no syntax to check itself).

### Task 2 — Add a webhook route skeleton [x]
Add `POST /api/webhooks/:provider` to `routes.js`. For now this just
needs to: log the raw body + headers, route to a (not-yet-implemented)
per-provider handler stub that returns 200 immediately, and store
nothing yet — this task is purely the routing skeleton so Tasks 3–6
below can each independently plug in one provider's real signature
verification + handling without fighting over the same file. Express's
default JSON body parser (`express.json()`, already set up in
`index.js`) re-serializes the body — note in a comment that this could
be a problem for HMAC verification if a provider's signature is
computed over the *raw* bytes (Paystack's docs explicitly say raw
body), and flag that `express.json({ verify: (req, res, buf) => { req.rawBody = buf } })`
or similar may be needed — but don't implement that yet, just leave
the note for whichever task hits it first.

**What was found / what changed:** Added `POST /api/webhooks/:provider`
to `routes.js`, plus a `webhookHandlers` map with one stub per provider
(paystack/korapay/juicyway/payscribe), each just logging
`formatPayload(req.body)` and returning `{ received: true }` — no
signature verification or storage anywhere yet, on purpose, so Tasks
3–6 can each land independently. The route itself logs the full
headers object and the sanitized body before dispatching, returns 404
for an unrecognized `:provider` segment, and otherwise always
responds 200 (nothing to reject on until a task adds real
verification — once one does, an invalid signature should return 401
instead of falling through to this 200, noted inline as a comment).
Left the raw-body note as a comment above the handler map, per the
task's own instruction, rather than wiring up
`express.json({ verify })` now — that's for whichever of Tasks 3–6
needs true raw bytes first (almost certainly Task 3, Paystack, per the
findings section). Verified with `node --check routes.js` and
`node --check index.js` (both pass; `index.js` wasn't touched but
re-checked since it imports `routes.js`).

### Task 3 — Paystack webhook: signature verification + handling [x]
Confirm the HMAC-SHA512 / `x-paystack-signature` scheme directly
against paystack.com/docs/payments/webhooks/ (already found via
primary source, per the findings section — this task is mostly
implementation, light re-verification). Implement it in the webhook
skeleton from Task 2, for the `paystack.success` (and relevant
failure) events. If Task 2's raw-body note turned out to matter, deal
with it here first since Paystack needs the true raw body.

**What was found / what changed:** Re-confirmed the scheme directly by
fetching paystack.com/docs/payments/webhooks/ rather than trusting the
earlier secondary-source note. Two things changed from what Task 2
assumed: (1) Paystack's own official example hashes
`JSON.stringify(req.body)`, not raw request bytes, so the raw-body
middleware flagged in Task 2 was **not** needed here — the
`express.json()` parse-then-reserialize round-trip is what Paystack's
own docs use — this is now the confirmed behavior for Paystack
specifically, not necessarily for the other three providers (Task 4-6
should each check independently, don't assume the same holds). (2)
The event name is `charge.success` (not `paystack.success` as this
task's own title text guessed) and there's no dedicated charge-failure
event in Paystack's list at all — failures just don't raise a webhook.
Implemented `verifyWebhookSignature(body, signature)` on the `Paystack`
class in `providers/paystack.js` (HMAC-SHA512 keyed with the secret
key, constant-time compare via `crypto.timingSafeEqual`, returns
`false` — not a throw — on missing signature or length mismatch so the
route can decide the HTTP response). Wired it into the `paystack`
webhook handler in `routes.js`: invalid/missing signature now throws
an error carrying `statusCode = 401`, which the route's catch block
(also updated to respect `error.statusCode` instead of hardcoding 500)
turns into a real `401` response instead of the previous unconditional
`200`. On a verified `charge.success`, logs reference/amount/status;
every other verified event type is logged generically since there's no
persistence layer yet (Task 12) and no other event needs action yet.
Updated the findings section above to "confirmed directly" with the
exact primary URL and fetch date, plus the full current event list for
future reference. Verified: `node --check routes.js` and
`node --check providers/paystack.js` both pass; also wrote a throwaway
`node -e` script exercising the HMAC logic against four cases (valid
signature accepted, tampered signature rejected, missing signature
rejected, wrong secret rejected) — all four passed — then deleted the
script per the process doc's instruction not to commit scratch files.

### Task 4 — Korapay webhook: confirm signature scheme + implement [x]
The `x-korapay-signature` / HMAC-SHA256 scheme in the findings section
above came from a secondary source — confirm it directly against
developers.korapay.com/docs/webhooks before implementing. Update the
"Confirmed research findings" section above to say "confirmed
directly" (with the exact primary URL) once you have, so future
sessions don't redo this. Then implement the handler.

**What was found / what changed:** Fetched
developers.korapay.com/docs/webhooks directly. The secondary-source
note was right about the algorithm (HMAC-SHA256) but incomplete on
scope: Korapay signs **only the `data` object**, not the full request
body — different from Paystack's Task 3 implementation, which signs
the whole body. Confirmed this distinction is load-bearing (not just
a documentation nuance) by hashing the same sample payload both ways
in a throwaway script and getting different hashes — a naive
full-body implementation copied from the Paystack pattern would have
silently rejected every real Korapay webhook. Implemented
`verifyWebhookSignature(body, signature)` on the `Korapay` class in
`providers/korapay.js`, hashing `JSON.stringify(body?.data)` (mirrors
Korapay's own official Node/PHP examples exactly), same
constant-time-compare pattern as Paystack's Task 3 implementation.
Wired it into the `korapay` webhook handler in `routes.js`: same
401-on-invalid-signature behavior as Paystack. On any of the six
confirmed event types (`charge.success`/`charge.failed`,
`transfer.success`/`transfer.failed`, `refund.success`/
`refund.failed`), logs reference/amount/currency/status; anything else
is logged generically. Updated the findings section above to
"confirmed directly" with the fetch date, the full event list, the
`data` object's documented fields, and the retry/response-code
behavior (always wants a `200`, retries up to 72h otherwise) for
future reference. Also confirmed the raw-body concern from Task 2
doesn't apply to Korapay either — its own official examples
re-serialize the parsed body just like Paystack's do. Verified:
`node --check routes.js` and `node --check providers/korapay.js` both
pass; a throwaway `node -e` script exercised valid/tampered/missing/
wrong-secret cases (all four correct) plus the full-body-vs-data-only
hash comparison — deleted after use, not committed.

### Task 5 — JuicyWay webhook: find the real scheme + implement [x]
Nothing about JuicyWay's webhook signature scheme has been found yet
at all. Start at docs.juicyway.com, find their webhooks page, document
what you find in the "Confirmed research findings" section above, then
implement.

**What was found / what changed:** Fetched
docs.juicyway.com/webhooks.md directly (found via
docs.juicyway.com/llms.txt's page index, since the bare `/webhooks`
path wasn't independently fetchable). Full scheme write-up is in the
"Confirmed research findings" section above — short version: no
signature header (checksum is a body field), HMAC key is a "business
ID" not the API secret, signed string is `event|alphabetically-sorted-
JSON(data)`, digest is uppercase hex. Implemented
`verifyWebhookSignature(payload)` on the `Juicyway` class in
`providers/juicyway.js` (takes the whole parsed body, not a header,
since the checksum lives inside it), plus a local `stableStringify()`
helper in the same file to produce the required alphabetical-key JSON
encoding. Added `this.businessId = process.env.JUICYWAY_BUSINESS_ID`
in the constructor — **this is a new required env var not previously
in this repo**; it is not the same as `JUICYWAY_API_KEY`/
`JUICYWAY_PUBLIC_KEY` and needs to be set wherever this app is
deployed for Juicyway webhook verification to work at all (currently
unset, `verifyWebhookSignature` will reject everything until it's
configured — flagging this plainly since it's a real deployment
prerequisite, not just a code change). Wired it into the `juicyway`
webhook handler in `routes.js`: same 401-on-invalid-checksum pattern
as Paystack/Korapay. On either of the two documented event types
(`payment.session.succeeded`/`payment.session.failed`), logs
reference/amount/currency/status; anything else is logged generically
(no persistence layer yet — Task 12). Updated the two stale routes.js
comments (above `webhookHandlers` and above the `/api/webhooks/:provider`
route) that still described Juicyway as an unverified stub. Left the
existing `⚠️ Verify exact endpoint path in Juicyway docs` comment on
`processPayment`'s `/v1/charges` call alone — that's a payment-init
endpoint question, not a webhook one, out of scope for this task (now
noted as a explicit future task in the findings section instead of
being silently left dangling). Verified: `node --check routes.js` and
`node --check providers/juicyway.js` both pass; a throwaway `node`
script exercised valid/tampered/missing-checksum/wrong-business-id
cases, sender key-order independence, and lowercase-checksum tolerance
(all six correct) — deleted after use, not committed. Pushed as part
of PR #2, **not yet merged by Phoenix-Boss** (see "Outstanding PRs
status" above — check it's still current as of whichever session reads
this next).

### Task 6 — Payscribe webhook: find the real scheme + implement [ ]
**Docs blocker resolved 2026-09-06 — the signature scheme itself is
now fully confirmed (doc-research only this session, no code changed;
see "Confirmed research findings" above, search "Payscribe — FULL API
discovery pass").** The PENDING_DOCS placeholder above has been
replaced by that full writeup. **Still blocked on implementation,
though, for two separate reasons, neither fixed by the docs alone:**
(1) we're still waiting on real API keys from Payscribe, so there'd be
nothing to exercise the verification code against in sandbox even
though the scheme is known — same "Fully tested with placeholder keys
means real sandbox credentials" rule Task 0/e states for every
provider; (2) the audit could **not** confirm the actual payload shape
of `accounts.payments.status` (the one webhook event this repo's
`processPayment` flow needs) — only its name, from the docs' sidebar
nav, not its body, which wasn't captured in the snapshot this audit
was built from. Implementing `verifyWebhookSignature` for Payscribe
(headers, HMAC-SHA256, replay window — all confirmed) can proceed
once keys exist; mapping the *payload* into Task 0/b-3's normalized
webhook envelope still needs that specific docs page opened first.
This is also the task that should revisit `Payscribe.verifyTransaction()`'s
current behavior (it just throws "requires Webhook or Bank Session ID"
today) — the audit found a real, named "Verify Payment" endpoint that
could make this stale, but couldn't confirm either way (its body
wasn't captured either); check that page directly before touching
this, don't assume the audit already settled it.

### Task 7 — Korapay: confirm the amount-unit question directly [x]
The findings section above has secondary evidence (decimal amounts in
docs examples) suggesting Korapay's `charges/initialize` wants base
units, not subunits — but this hasn't been confirmed against the exact
reference page for that specific endpoint. Open
developers.korapay.com's charges/initialize reference directly, find
an explicit statement or a clean non-ambiguous example, and update the
findings section to either confirm or correct this. If it turns out
subunits ARE required, fix `providers/korapay.js` to call
`toSubUnit()` (after also fixing Task 9 below, since the current
`toSubUnit()` map doesn't cover Korapay's full currency list).

**What was found / what changed:** Fetched
developers.korapay.com/docs/checkout-redirect directly — this is the
guide that documents the `charges/initialize` endpoint end to end,
including its full parameter table. The `amount` parameter is typed
`Integer` with no subunit/multiplier instruction anywhere on the page,
unlike Paystack's docs which explicitly say to multiply by 100.
Cross-checked against three more primary/near-primary sources (Checkout
Standard widget's `amount: 22000`/`amount: 3000` NGN examples, the
official Elixir client's `"amount" => "1000.00"` decimal-formatted
output, and the checkout-redirect page's own webhook payload example)
— all consistent with base currency units, none suggesting kobo. Base
units is now **confirmed directly**, not secondary-source inference.
No code change was needed: `providers/korapay.js#processPayment`
already forwards `data.amount` unconverted, which is the correct
behavior. Updated the "Confirmed research findings" Korapay section
above with full source detail so a future session doesn't need to
re-derive this. Verified with `node --check providers/korapay.js` (no
code touched, but re-checked since the task could have required a
change). Per the "Current focus: Korapay only" note above, this was
the one task worked this session — no other provider's task was
started.

### Task 8 — Paystack: verify endpoint paths + response shape against docs [x] (doc-only; end-to-end exercise still blocked on keys)
**Done this session (2026-09-06), doc-research half only** — this
task's own prior note already carved out that exception ("Doc
research alone doesn't need a key, so a future session could still do
the read-only confirmation half if useful"), so this pass took that
option instead of skipping the task entirely. **Still blocked on
Paystack API keys for actually exercising any of this end-to-end** —
see "Current focus: Korapay only" above, unchanged by this pass.

Both `/transaction/initialize` and `/transaction/verify/:reference`
are **confirmed exact, byte-for-byte correct** against
paystack.com/docs/api/transaction/ — no path or method changes
needed. The response shape this repo relies on (`responseData.status`
at the top level; `data.authorization_url`/`data.access_code`/
`data.reference` for initialize) also matches exactly.

This pass went beyond just those two endpoints and beyond "not
expected to find much" — it turned into a **full API-discovery pass**
(currencies, error format, rate limits, webhook signature
re-confirmation, and every other Transaction-API endpoint Paystack
documents, even the ones this repo doesn't call). Full writeup moved
to the "Confirmed research findings" section above (search for
"Paystack — FULL API discovery pass"), matching the precedent already
set there for Korapay's full pass. **Two real findings came out of
it, queued as their own tasks rather than fixed under this task's own
doc-only scope:**
- **Task 8c** — `GET /api/verify` reports `"Verification successful"`
  for transactions that actually failed or were abandoned, because
  neither `providers/paystack.js#verifyTransaction` nor the route
  itself checks the nested `data.status` field, only the top-level
  API-call-succeeded `status` field. Real bug, not a docs mismatch.
- **Task 8d** — the confirmed-currency list for Paystack
  (`utils/helpers.js`) is missing `XOF`, which Paystack's own docs
  list as a sixth supported currency.

### Task 8c — Fix `GET /api/verify`: surface Paystack's real per-transaction status, not just the API-call status [ ]
**Added by Task 8's full audit pass (2026-09-06), doc-research only —
not fixed as part of that pass since it's a code change outside
Task 8's own stated scope, and this repo is still under the "Current
focus: Korapay only" narrowing above (this is a Paystack-specific fix,
not Korapay).** Confirmed directly against
paystack.com/docs/api/errors/: Paystack always answers a verify call
with HTTP 200 and top-level `status: true` **even when the underlying
transaction failed or was abandoned** — "Note that we will always
send a 200 if a charge or verify request was made. Do check the data
object to know how the charge went." The transaction's real outcome
only lives in the nested `data.status` field (`"success"` /
`"failed"` / `"abandoned"`).

`providers/paystack.js#verifyTransaction` only throws on
`!response.ok || !responseData.status` — both of which stay
true/truthy for a failed-or-abandoned transaction — so it returns
normally. `routes.js`'s `GET /api/verify` route then unconditionally
answers `{ status: true, message: 'Verification successful', ...,
data: result }`, with the real outcome buried in
`data.data.status` and never checked or surfaced at the top level. A
caller reading the natural top-level `status`/`message` fields (which
is what this route's own response shape invites) would treat a
declined card or an abandoned checkout as a confirmed payment — the
exact "paid but no value" failure mode Paystack's webhooks guide
warns integrators about.
**Fix, once picked up:** in `verifyTransaction`, either (a) also throw
(or return an explicit failed-status result) when
`responseData.data?.status !== 'success'`, or (b) leave the provider
method as a thin pass-through and instead fix it one layer up in
`routes.js`'s `/verify` handler, setting the route's own top-level
`status`/`message` from `result.data.status` rather than hardcoding
`status: true, message: 'Verification successful'` whenever no
exception was thrown. Either approach needs a decision on which layer
owns "did the payment actually succeed" — flagging that decision
here rather than presupposing it.
**Blocked on Paystack keys for end-to-end testing**, same as Task 8
itself — but this is a pure logic fix (no API call shape changes), so
it can be written and unit-reasoned-about without live keys; only the
final end-to-end confirmation needs them.

### Task 8d — Add `XOF` to Paystack's confirmed-currency list [ ]
**Added by Task 8's full audit pass (2026-09-06), doc-research only —
not fixed as part of that pass since editing
`CONFIRMED_PROVIDER_CURRENCIES` is Task 9/9b's territory (currency-list
work), not Task 8's (endpoint/shape verification).** Confirmed
directly against paystack.com/docs/api/ ("Supported currency" table):
Paystack supports **six** currencies, not five — `NGN`, `USD`, `GHS`,
`ZAR`, `KES`, and **`XOF`** (West African CFA Franc, Côte d'Ivoire).
`utils/helpers.js`'s `CONFIRMED_PROVIDER_CURRENCIES.paystack` lists
only the first five. This isn't a mistake in how the prior list was
built — its cited sources (Chargebee, Zoho, mctaba.com) are
third-party integration guides that only describe Paystack's five
better-known currencies — but Paystack's own primary docs now list a
sixth.
**Fix, once picked up:** add `'XOF'` to the array. **No multiplier
change needed** — Paystack's docs explicitly say "While there is no
subunit for XOF, developers must multiply the amount by 100
regardless," so the existing uniform `{ unit: 'subunit', multiplier:
100 }` in `getAmountFormat('paystack', ...)` is already correct for
XOF too; this is a pure addition to the array, not a new branch.
Not blocked on API keys (this is a static list, not a call this repo
makes) — could be picked up any time regardless of the "Current
focus: Korapay only" narrowing's usual rule, since it's a one-line,
already-fully-confirmed change with no ambiguity to resolve. Left
unchecked/undone here anyway, per this session's scope being
doc-only, not because it's blocked.

### Task 8b — Juicyway: verify the payment-initialization endpoint path [x] (doc-only; confirmed WRONG, not fixed here)
**Resolved this session (2026-09-06) by a full API-discovery pass —
see "Confirmed research findings" above ("JuicyWay — FULL API
discovery pass").** The endpoint path is **confirmed wrong**:
`/v1/charges` doesn't exist in JuicyWay's documented API; the real
endpoint is `POST /payment-sessions`. This pass went well beyond just
the endpoint path (same "turned into a full audit" pattern as Task 8's
Paystack pass) and found three more real, confirmed bugs plus one
unresolved documented ambiguity — none fixed here, since this remains
a doc-research pass per the Discovery Convention. Each is queued as
its own task immediately below (**Tasks 45a–45e**) rather than fixed
under this task's own now-narrower scope.

### Task 45a — Fix JuicyWay's endpoint path and Authorization header format [ ]
**Added by Task 8b's full audit pass (2026-09-06), doc-research only.**
Two confirmed bugs, fixed together since neither alone gets a real
call to succeed: (1) `providers/juicyway.js#processPayment` calls
`POST ${baseUrl}/v1/charges` — change to `POST ${baseUrl}/payment-sessions`.
`verifyTransaction` calls `GET ${baseUrl}/v1/charges/${reference}` —
change to `GET ${baseUrl}/payments/{id}` (note: takes JuicyWay's own
`id`, not the merchant `reference` — see Task 45d, this may need to
land together with or after that task, not independently). (2) Both
methods send `'Authorization': \`Bearer ${this.apiKey}\`` — remove the
`Bearer ` prefix entirely; docs.juicyway.com/authentication.md is
explicit that the header is the raw key with no scheme prefix. Not
blocked on API keys for writing the fix (this is a pure string/path
change, confirmed against docs), but end-to-end confirmation still
needs real JuicyWay sandbox keys, same as every other JuicyWay task.

### Task 45b — Fix JuicyWay's request payload shape (missing required nested fields) [ ]
**Added by Task 8b's full audit pass (2026-09-06), doc-research only.**
`providers/juicyway.js#processPayment` currently builds `{ amount,
email, reference, currency }` — the documented required body is `{
amount, currency, description, reference, payment_method: { type:
"card" }, order: { identifier, items: [{ name, type }] }, customer: {
email, first_name, last_name, phone_number, billing_address, type,
ip_address } }`. This is a bigger change than Task 45a: it changes
this repo's own `processPayment(data)` contract, since callers
(`routes.js`) would need to supply first/last name, phone, billing
address, customer type, IP address, an order identifier/items array,
and a description — none of which Paystack/Korapay's simpler
`{ amount, email, reference, currency }` shape requires today.
Whoever picks this up needs to decide whether `routes.js`'s existing
`/api/pay`-style request body grows JuicyWay-specific optional fields,
or whether JuicyWay processing gets its own route/validation path —
a real design decision, not just a payload literal to edit.
**Blocked on that design decision plus JuicyWay sandbox keys for
end-to-end confirmation.**

### Task 45c — Fix JuicyWay's error-message extraction (reads the wrong field, always falls back to a generic string) [ ]
**Added by Task 8b's full audit pass (2026-09-06), doc-research only.**
`providers/juicyway.js` reads `responseData.message` in both
`processPayment` and `verifyTransaction` — JuicyWay's real error
envelope is `{ error: { code, message, type, details } }`, so the
real message lives at `responseData.error?.message`. As currently
coded, `responseData.message` is `undefined` for every real JuicyWay
error, so the hardcoded fallback string (`'Juicyway payment failed'` /
`'Juicyway verification failed'`) fires on every single failure,
discarding JuicyWay's own specific, documented-safe-to-surface message
(e.g. "Amount must be at least 100000") every time. **Fix:** change
both to `responseData.error?.message || 'Juicyway ... failed'`. Not
blocked on API keys to write (the shape is confirmed from docs), but
real-error confirmation needs a live failing call. Also worth deciding
alongside this fix, not blocking it: whether to also surface
`responseData.error?.code` for programmatic branching (same open
question Task 8's Paystack audit raised for `type`/`code` there, and
DodoPayments' audit raised again for its own `code` field) or a 402
card-decline branch specifically (JuicyWay documents `card_declined`
as its own status code, distinct from generic 4xx).

### Task 45d — Design and implement JuicyWay's reference-vs-ID verify flow [ ]
**Added by Task 8b's full audit pass (2026-09-06), doc-research only.**
Real architectural gap, not a one-line fix: JuicyWay's `GET
/payments/{id}` (Fetch Payment) takes JuicyWay's own UUID
(`data.payment.id` from the initialize response), and List Payments'
documented filters don't include lookup-by-merchant-reference. This
repo's `verifyTransaction(reference)` signature assumes
reference-based lookup works, matching how Paystack/Korapay's own
verify calls work — that assumption is confirmed false for JuicyWay
specifically. Whoever implements this needs to decide how the
JuicyWay-issued `id` gets from the `processPayment` response through
to whatever later calls `verifyTransaction` — e.g. `processPayment`
returning `id` alongside its existing response so the caller can pass
it back in, versus some other propagation path — before Task 45a's
verify-endpoint fix can be meaningfully exercised end-to-end. Depends
on this design decision landing before (or together with) Task 45a's
verify-path change.

### Task 45e — Resolve JuicyWay's three-way currency-list conflict before adding it to `CONFIRMED_PROVIDER_CURRENCIES` [ ]
**Added by Task 8b's full audit pass (2026-09-06), doc-research only.**
JuicyWay's own docs give three different answers for supported
currencies across three pages (NGN+CAD only; NGN/USD/CAD/USDT/USDC;
NGN/USD/CAD in one page's own error example) — see the "Currencies"
finding in the audit above for the exact sources. **Do not add a
`juicyway` entry to `CONFIRMED_PROVIDER_CURRENCIES` based on any one
of these three lists** — `getAmountFormat` already throws for
`juicyway` today specifically to prevent a silent wrong guess on real
money, and this conflict is exactly the situation that guard exists
for. Resolve by testing a real sandbox call (e.g. attempt a session in
USDT and see whether it's accepted or 422s) once JuicyWay sandbox keys
are available, not by picking whichever of the three documented
answers seems most authoritative. The same page's minimum-amount
figures also disagree by a factor of 1,000 ("Minimum: 100" vs. "Amount
must be at least 100000") — resolve both together, same sandbox test
can likely answer both.

### Task 9 — Expand currency/amount-unit handling per real provider capabilities [ ]
Depends on Tasks 3–8 having established real per-provider currency
lists and amount-unit rules. Rework `toSubUnit()`/`fromSubUnit()` in
`utils/helpers.js` so the unit conversion is applied **per provider**,
not blanket — e.g. a `getAmountFormat(provider, currency)` helper each
provider file calls, instead of one global assumption. Also expand the
currency list itself: don't hardcode a guessed "25 currencies" number
— pull the real, current country/currency list from the Mavins-web
repo (`src/lib/campaign/geoAffinity.ts`'s `TARGET_COUNTRIES`, and
`COUNTRY_CURRENCY` in `src/app/promote/page.tsx` — note these two
lists didn't even match each other as of the last Mavins-web session,
14 vs 20 entries respectively; reconciling that mismatch may itself
need to happen on the Mavins-web side, not here — just don't invent a
currency list here that doesn't correspond to something real on that
side).

**Partial progress this session (Korapay-only focus, box left
unchecked — task split, see below):** Implemented the
`getAmountFormat(provider, currency)` helper design exactly as this
task describes, plus a `convertAmountForProvider(amount, provider,
currency)` wrapper, both in `utils/helpers.js`. Filled in real rules
for the two providers with confirmed research: **Paystack** (subunit,
×100, 5-currency list — this was already confirmed pre-existing
research, not new this session) and **Korapay** (base unit, no
multiplier, 9-currency list — confirmed this session via Task 7).
**JuicyWay and Payscribe intentionally throw** a clear "not yet
confirmed" error instead of guessing a multiplier — per this project's
"Current focus: Korapay only" note above, their amount-unit rules
haven't been separately verified and we don't have working keys to
test against even if we guessed right. Rewired `providers/paystack.js`
(now calls `convertAmountForProvider` instead of the old direct
`toSubUnit` call) and `providers/korapay.js` (now explicitly calls
`convertAmountForProvider` too — a confirmed no-op, but it makes the
Task 7 rule enforced in code, not just documented in a comment) —
`providers/juicyway.js` and `providers/payscribe.js` were **not**
touched, since routing their existing `data.amount` pass-through
through the new helper would just throw for both of them right now,
which would be a regression, not an improvement, until Task 6/8-style
confirmation happens for each. Left the old `toSubUnit()`/
`fromSubUnit()` functions in place (unused by any provider file now,
but not deleted — a future session can remove them once nothing else
might reasonably want the raw ×100 utility, or repurpose them inside
`getAmountFormat`'s own subunit case). Verified: `node --check` on all
three touched files, plus a throwaway sanity script (deleted after)
exercising Paystack/Korapay conversion math, the JuicyWay/Payscribe/
unknown-provider throw paths, and `getAmountFormat`'s return shape —
all passed.
**Why the box stays unchecked:** the currency-list-expansion half of
this task (pulling the real list from Mavins-web) was **not**
attempted — split out explicitly into the new **Task 9b** immediately
below, per this project's own "split a task, don't half-finish it
silently" rule. **A future session that reaches this unchecked box
should skip straight to Task 9b rather than redoing the
`getAmountFormat`/`convertAmountForProvider` work above** — that part
is done. Task 9's own box should probably be considered done in spirit
for the two providers we can currently test, but stays unchecked until
either (a) JuicyWay/Payscribe get their own confirmed rules and the
helper covers all four providers, or (b) the project owner decides the
currency-list-expansion clause doesn't actually block calling it
complete — that's a scope call for the project owner, not this
session, to make.

### Task 9b — Pull the real currency list into `getAmountFormat` from Mavins-web [x]
Split off from Task 9 above (see its note) rather than left half-done
in the same task. Depends on **Mavins-web's currency-list
reconciliation** (this file's own Task 18, under "Cross-repo
continuation" below — confusingly *not* a task with that number inside
Mavins-web's own `handover.md`, which has an unrelated Task 18; see
that entry's "Status check" note for the correction) being done first —
check that entry's current state before starting this one; if it isn't
done yet, this task isn't ready either, skip it same as any other
blocked task.

**Was blocked, confirmed unblocked and completed this session
(2026-08-27):** Mavins-web's own handover.md top box reported "Task
9b is now unblocked — Task 29's reconciled
`src/lib/currency/countryCurrency.ts` is the real currency list that
task needed to pull into `getAmountFormat`." Cloned Mavins-web fresh
and read that file plus its companion
`src/lib/currency/korapayDccCurrency.ts` directly to verify rather than
trust the pointer alone. Finding: `korapayDccCurrency.ts`'s own doc
comment already cites this repo's Task 7 research as *its* source
("B-Pay-backend's handover.md, Task 7") — so the two repos' Korapay
currency lists were derived from the same original research, not two
independently-arrived-at lists that happened to need reconciling.
Cross-checked the actual values anyway rather than assuming the
citation meant they'd stayed in sync: both list exactly `NGN, GHS,
KES, ZAR, USD, XAF, XOF, EGP, TZS` — identical, no drift. **No values
changed in `utils/helpers.js`'s `CONFIRMED_PROVIDER_CURRENCIES` as a
result** — this task turned out to be a confirmation, not a
correction. Updated that constant's comment in `utils/helpers.js` to
record the cross-check (so a future session doesn't have to re-derive
that these two repos agree) rather than leave the values unchanged
with no trace that this check happened.
**Related finding, noted but correctly out of scope for this specific
list:** Mavins-web's `countryCurrency.ts` also revealed that of its 25
target countries, only 8 (NG, GH, KE, ZA, EG, TZ, CI, SN) can actually
be charged via Korapay's Dynamic Currency Conversion today — the other
17 get a display-only currency estimate but are still charged in
NGN/USD. This is a real, already-flagged gap on the Mavins-web side
(that repo's own file says closing it means either Korapay adding DCC
support for more currencies, or a second payment provider for those
markets — see its Task 30) — it doesn't change anything about *this*
repo's `CONFIRMED_PROVIDER_CURRENCIES`, which is about what currencies
Korapay's API will accept at all, not which countries get DCC. Noted
here only so a future session doesn't rediscover it from scratch.
Verified: `node --check` on `utils/helpers.js`, `routes.js`,
`providers/korapay.js`, `providers/paystack.js` (all files that import
from `helpers.js`), plus ran `getSupportedCurrencies('korapay')` and
`getSupportedCurrencies('paystack')` directly to confirm the returned
arrays are unchanged and correct.

### Task 10 — Currency/country/method-aware provider routing [ ]
Replace `ROUTING_RULES`'s abstract `action` string with real routing:
given a currency (and ideally a country code, if the caller has one),
pick a provider that actually supports it, per the findings above
(Paystack: NGN/GHS/ZAR/KES/USD only. Korapay: broader list, channel
varies by country — mobile money vs bank transfer vs card. Payscribe:
appears NGN-only based on its `.ng` sandbox domain — confirm, don't
assume. JuicyWay: check its real currency/country coverage once Task 5
research is done). If nothing supports a given currency, return a
clear 4xx error naming the currency, not a silent fallback to
`paystack` (today's default fallback, which would likely just fail
downstream with a confusing provider-side error instead of a clear
one).

**Partial progress this session (Korapay-only focus, box left
unchecked — same pattern as Task 9's partial):** `ROUTING_RULES` itself
is still untouched — a full currency-aware provider *selection* across
all four providers needs all four to have a confirmed currency list
first, and JuicyWay/Payscribe don't yet (same blocker Task 9 hit).
What this session added instead: `getSupportedCurrencies(provider)` in
`utils/helpers.js` (extracted from the currency arrays that were
already inline inside `getAmountFormat`'s Paystack/Korapay cases —
`getAmountFormat` now calls it too, so there's one list per provider,
not two copies that could drift apart), and a new
`assertCurrencySupported(providerName, currency)` in `routes.js`,
called in `POST /pay` right after a provider is resolved (whether via
explicit `provider` or via `action` → `ROUTING_RULES`) and before
`getProvider()`/`processPayment()` are ever reached. For Paystack and
Korapay (the two providers with a confirmed list), a currency outside
that list now gets a clear `400` naming both the currency and the
provider, instead of being forwarded to reach the provider's API and
fail there with a less obvious error (or, worse, silently going
through if the provider's API doesn't itself reject it). For JuicyWay
and Payscribe, `getSupportedCurrencies()` returns `null` and
`assertCurrencySupported` treats that as "can't validate yet" and lets
the request through unchanged — same behavior as before this task,
not a regression, per the same "don't guess" principle Task 9 applied
to their amount-unit rules. Also fixed a real, previously-unrelated bug
found while wiring this in: `POST /pay`'s `catch` block always
answered `500` regardless of `error.statusCode` — unlike the
`/webhooks/:provider` route, which has respected `error.statusCode`
since Task 3. That meant this task's new 400 would have silently come
back as a 500 without this fix, so it's included in the same commit
rather than filed separately. Verified: `node --check routes.js` and
`node --check utils/helpers.js` both pass; a throwaway `node -e` script
(deleted after) exercised `getSupportedCurrencies` for all four
providers (correct lists for Paystack/Korapay, `null` for the other
two) and `assertCurrencySupported` against six cases — Korapay+NGN
(pass), Korapay+CAD (400), Paystack+GHS (pass), Paystack+XOF (400),
JuicyWay+anything (pass-through), Payscribe+anything (pass-through) —
all six matched expectation.
**Why the box stays unchecked:** `ROUTING_RULES`'s actual provider-
*selection* logic (as opposed to this session's after-the-fact
validation of whatever it already picked) is still the abstract
4-action map from before — the real "pick a provider given a
currency+country" rework this task describes needs JuicyWay and
Payscribe's currency lists confirmed first (JuicyWay: no task has
checked this yet; Payscribe: blocked on PENDING_DOCS, see above), so a
future session should pick this up once "Current focus: Korapay only"
is lifted, not treat this partial pass as the finished task.

### Task 11 — Request validation on POST /pay [x]
Validate: `amount` is a positive number, `currency` is a 3-letter code
present in whatever the Task 9/10 currency tables end up being,
`customer.email` is present and looks like an email when the target
provider requires one (Paystack and Korapay do; check others).
Malformed requests should get a clear 400 with a specific message, not
fall through to a provider API call that fails confusingly. Keep this
dependency-free (no new npm package needed) unless the validation
logic gets unwieldy as plain JS — if so, `zod` is a reasonable, small
addition; note the choice either way in the commit message.

**What was found / what changed:** Kept this dependency-free — plain
regex/type checks, no `zod` needed, the logic stayed small. Added
three new checks to `utils/helpers.js`: `isValidCurrencyCode()`
(3-letter format, case-insensitive), `isValidEmail()` (basic
`x@y.z`-shape check, not a full RFC 5322 validator — good enough to
catch typos/empty strings without being its own project), and
`providerRequiresEmail()`. That last one required reading all four
providers' `processPayment()` call sites directly rather than assuming
just Paystack/Korapay per the task's own hint text: **JuicyWay also
requires email** (forwards `data.customer?.email` with no fallback,
same shape as Paystack) — this wasn't previously called out anywhere
in the findings section, so it's new information from this task, not
just implementation. **Payscribe does NOT require it** at this
layer — its `processPayment()` already defaults to a placeholder
`'customer@example.com'` when none is given, so enforcing a real email
for Payscribe here would be inventing a stricter requirement than the
code actually has (flagging Payscribe silently accepting a fake email
as its own separate, pre-existing concern — not fixed as part of this
task, since that's a Payscribe-provider-file change, not a
request-validation one).
In `routes.js`: `assertValidAmount()` (rejects non-number, non-finite,
zero, and negative — the old `if (!amount)` check let a numeric-string
`"100"` or `NaN` through silently) and `assertValidCurrencyFormat()`
(shape-only 3-letter check, run for every request regardless of
provider) both run immediately after logging the incoming request,
before any provider routing happens. `assertValidCustomerEmail()` runs
right after `assertCurrencySupported` (Task 10), once `providerName`
is known, since whether email is required depends on which provider
got picked. All three throw an `Error` with `.statusCode = 400` and a
specific message naming the bad field and what was received — caught
by the same `error.statusCode || 500` catch-block fix Task 10 already
made, so these come back as real 400s, not 500s.
Deliberately NOT done here (would be scope creep / a different task):
cross-checking `currency` against the *resolved provider's* actual
supported list — that's already `assertCurrencySupported`'s job
(Task 10); this task's currency check is shape-only ("does it look
like a real code"), not a whitelist check, per the task's own
"present in whatever the Task 9/10 currency tables end up being"
framing, which reads as "consistent with those tables' format," not
"re-implement the same lookup twice."
Verified: `node --check routes.js` and `node --check utils/helpers.js`
both pass. A throwaway `node -e` script (deleted after use) exercised
`isValidCurrencyCode` (9 cases: valid/lowercase/word/numeric/empty/
undefined/valid/2-letter/4-letter — all correct), `isValidEmail` (6
cases: valid/invalid/empty/undefined/missing-TLD/valid-with-subdomain —
all correct), `providerRequiresEmail` for all four providers (correct
per the note above), and `assertValidAmount`'s logic standalone (7
cases: valid/zero/negative/numeric-string/NaN/Infinity/small-decimal —
all correct).

### Task 12 — Idempotency protection [x]
This one needs a decision, not just code: this repo currently has no
persistence layer at all (no database). A real fix needs *somewhere*
to remember "we already processed reference X" across requests — that
might mean adding a lightweight store here (even a simple JSON file or
SQLite for a start), or it might mean this responsibility actually
belongs on the Mavins-web/Supabase side (which already has a database)
and this backend should just accept a client-supplied idempotency
key and pass it through to the provider where each provider's own API
supports one, without needing its own storage. **Don't build a database
layer without deciding this first** — if it's ambiguous, do the
smaller, in-scope half (accept and forward a client idempotency key
where providers support one) and leave a clear note in this file
under "Known issues" recommending the human decide the storage
question, rather than guessing at a bigger architecture change.

**What was found / what changed:** Did the smaller, in-scope half only,
per the task's own instruction — the storage decision (own DB here vs.
Mavins-web/Supabase side vs. accept-and-forward-only) is a real
architecture call for the project owner, not something to guess at, so
it's left open below rather than acted on. What this session confirmed
and built: `POST /pay` already destructured a client-supplied
`reference` from the request body and forwarded it as-is
(`ref = reference || generateReference(providerName)`) — so
accept-and-forward already existed structurally before this task; what
it was missing was any validation of that client-supplied value before
forwarding it. Added `assertValidReferenceFormat(providerName,
reference)` to `routes.js`: if the client omits `reference` entirely,
this is a no-op (the existing `generateReference()` fallback already
produces something safe). If they supply one, it must be a non-empty
string, and — for Paystack specifically — must match the character set
its own docs require (confirmed directly against
paystack.com/docs/api/errors/transaction/: "Only -,.,= and
alphanumeric characters are allowed"); a client-supplied reference with
e.g. a `#` or space in it now gets a clear 400 naming the bad character
instead of reaching Paystack and failing there with its own less
specific error. Korapay's own primary docs
(developers.korapay.com/docs/checkout-redirect) only state the
reference "Must be unique for every transaction" — no character
restriction — so no charset check is applied for Korapay beyond
non-empty-string; JuicyWay/Payscribe: same (no format research done
this session, out of the current Korapay-focus scope).
**Important correction to a claim NOT in this file before:** while
researching this, a secondary source (a third-party "skills" listing
aggregating Korapay's API, not developers.korapay.com itself) claimed
Korapay's reference is "idempotent" in the strong sense — that
resending the same reference "returns the original charge" (a cached
result, no error). This was checked directly against Korapay's own
primary docs this session and is **NOT confirmed there** — the
primary source only states the *uniqueness requirement*, worded
almost identically to Paystack's (which is documented, also via a
primary source, to reject a reused reference outright with a
"Duplicate Transaction Reference" error, not return a cached result).
Recording this here so a future session doesn't accidentally treat
the unconfirmed secondary claim as fact: **the safer, primary-source-
backed assumption is that both Paystack and Korapay reject a reused
reference as an error**, not that either silently returns a cached
prior result. If a future task actually needs true idempotent-replay
semantics (client retries the exact same request and gets the exact
same response back, no error), that requires the storage layer this
task explicitly declined to build — see the "Known issues" note below.
Verified: `node --check routes.js` and `node --check utils/helpers.js`
(routes.js was the only file touched, but `utils/helpers.js` was
re-checked since it's imported). A throwaway `node -e` script (deleted
after use) exercised `assertValidReferenceFormat` against 8 cases
(omitted reference / valid Paystack reference / invalid-character
Paystack reference / empty string / non-string / Korapay with
special characters allowed through / Korapay empty string rejected /
JuicyWay pass-through) — all eight matched expectation.
**(Originally left unchecked pending the storage/architecture decision
below — resolved and box now checked, see this session's note
immediately after.)**

**Update — decision received, see "Project owner decisions" → Decision
1 near the top of this file:** the owner picked option (b) — reference
storage and idempotency live on the Supabase side via an Edge Function,
not in this backend. This task's in-scope half (validate + forward a
client-supplied reference) already matches that decision and needs no
further code change here. What's now unblocked is a *new* task — Task
23 below — to confirm every caller actually sends its own reference
going forward, since the decision assumes that, rather than leaning on
this backend's own `generateReference()` fallback.

**This session (2026-08-27):** picked this up as the next actionable
item in queue order — Tasks 6 and 8 are explicitly on hold, Task 9's
own note says to skip straight to 9b, 9b is confirmed still blocked on
Mavins-web's currency-list reconciliation, and Task 10's remaining
`ROUTING_RULES` rework needs JuicyWay/Payscribe currency lists this
backend doesn't have yet — none of that left anything actually
actionable ahead of this task. Re-read `routes.js` directly (not just
this file's prior notes) to confirm the two paragraphs above weren't
just stale prose: `assertValidReferenceFormat(providerName, reference)`
is present and wired into `POST /pay` exactly as described (no-op on
omitted reference, non-empty-string check, Paystack charset check),
matching Decision 1's resolution with nothing left to build here. The
two paragraphs directly above had been left contradicting each other —
one said the box stays unchecked pending a decision, the next said the
decision arrived and needs no further code — so this was a stale
bookkeeping gap, not an actual open task. No source file changed this
session; only this file (checkbox + the note you're reading). Per this
project's own rule, non-code (docs-only) sessions still get their own
commit and patch — see `0012` in "Patches issued so far" below.

### Task 13 — Basic security hardening [x]

**Rate-limiting remainder resolved as won't-fix, not left open —
correcting this session:** the error-handling-review half was already
done (see the full write-up below). The only remaining scope, rate
limiting, was **explicitly declined by direct project owner
instruction** ("no need for rate limiting") — that's a real decision,
not a blocker, so this box should reflect done, not pending. If the
project owner ever reverses that instruction, reopen as a new task
rather than un-checking this one.
Add rate limiting on `POST /api/pay` and `POST /api/webhooks/:provider`
(e.g. `express-rate-limit`, a small dependency). Review every
provider's error handling for anything that might leak upstream
details (API keys, internal codes, stack traces) into the client-facing
error message, and sanitize where needed.

**Partial progress this session (rate limiting explicitly descoped —
project owner instruction, not a session decision — box stays
unchecked, see below):** Did the error-handling-review half only.
Read every provider file's `processPayment()`/`verifyTransaction()`
plus `routes.js`'s three catch blocks (`POST /pay`, `GET /verify`,
`POST /webhooks/:provider`) end to end. Found two distinct kinds of
error messages currently reaching the client, previously handled
identically:
1. **Provider-authored, user-facing messages** — e.g. Paystack/Korapay/
   Juicyway's `responseData.message`, Payscribe's `responseData.description`
   (a "insufficient funds"/"invalid account" style string the provider
   itself designed to be shown to an end user). Safe to pass through,
   and the whole point of surfacing them.
2. **Internal/operational messages** — network failures inside
   `fetch()` (DNS errors, connection resets), JSON-parse failures on a
   non-JSON response, and — the one worth calling out — `getProviderKey()`/
   `getProviderBaseUrl()` in `utils/helpers.js` throwing "API key not
   found for X" / "No base URL configured for X" when a provider isn't
   configured on this deployment. That second one is a real, if minor,
   information-disclosure gap: it was reaching the client verbatim,
   telling any caller exactly which providers this server does or
   doesn't have live credentials for — deployment/readiness state that
   has no reason to be public. (Deliberately did NOT flag "Provider 'x'
   not supported" / "Unsupported provider" messages the same way —
   those just restate which provider names are valid, no more sensitive
   than the API's own documented provider list.)
Fixed by adding `providerError()` in `utils/helpers.js` (wraps a
message and tags it `isProviderMessage = true`), used at all 7
provider-response-failure throw sites across the four provider files
in place of a bare `new Error(...)`. `handleApiCall()` (also in
`utils/helpers.js`) now only passes a caught error's message through to
the client verbatim when it carries that flag — anything else becomes
a generic `Unable to complete request with <provider> right now...`,
while the real message still goes to the server log line immediately
above (unchanged). Separately, `getProviderKey()`/`getProviderBaseUrl()`
now tag their throws `isConfigError = true`; added a shared
`clientSafeMessage(error, fallback)` helper in `routes.js`, used by all
three catch blocks, which swaps in a generic "Payment service is
temporarily unavailable for this provider" message whenever that flag
is present. Full detail is unaffected in every case — only the
*client-facing* `message` field changes; server-side `log(...)` calls
still get the real error text everywhere.
**Incidental fix found while reviewing, included in the same commit:**
`GET /verify`'s catch block always answered `500` regardless of
`error.statusCode` — the same pre-existing bug pattern Task 10 already
fixed for `POST /pay` (and `POST /webhooks/:provider` has respected it
since Task 3), just never applied here. Now consistent across all
three routes.
Verified: `node --check` on all six touched files (`routes.js`,
`utils/helpers.js`, all four provider files) — all pass. A throwaway
`node -e`-style script (deleted after use) exercised `handleApiCall`
with a `providerError()`-flagged throw (message passed through
verbatim) and an unflagged `TypeError` (message replaced with the
generic one), plus `clientSafeMessage()` against a tagged config error
(sanitized) and an untagged validation error (passed through) — all
four matched expectation.
**Why the box stays unchecked:** rate limiting was explicitly descoped
for this session per direct project owner instruction ("no need for
rate limiting"), not skipped for a code reason — it's still real,
unaddressed scope from this task's original description (no rate
limiting exists anywhere in this repo). A future session should pick
up just that half; the error-handling-review half above doesn't need
to be redone.

### Task 14 — End-to-end manual test pass [ ]
Using each provider's sandbox/test keys, exercise `/api/pay` and
`/api/verify` (and by this point, the webhook handlers) for all four
providers. Write down what you tested and the result as a short
`TESTING.md` (or append to this file) — this is a manual pass, not an
automated test suite (no test framework is set up in this repo yet;
adding one is out of scope unless a future task specifically calls
for it). Confirm `/health`'s provider-key check reflects reality.

### Task 15 — Final audit pass before handoff to Mavins-web [x]
Re-read all four provider files and `routes.js` end to end. Confirm
every `⚠️` / TODO-style comment from the original code has either been
resolved or turned into a tracked task above. Confirm the "Confirmed
research findings" section is fully up to date (no more "secondary
source, not yet confirmed" caveats left for anything that got used in
shipped code). This is the last B-PAY-backend-only task — Task 16
onward switches repos.

**What was found / what changed:** Grepped all four provider files,
`routes.js`, `utils/helpers.js`, and `index.js` for `⚠️`/TODO/FIXME/XXX
and for looser uncertainty language (confirm/verify/assume/guess/not
sure), then read every hit in context rather than trusting the grep
alone. Most hits were either legitimate runtime warning log lines (not
TODOs) or already-resolved decisions with their reasoning documented
inline (e.g. the Korapay-idempotent-reuse secondary-source correction
from Task 12, already recorded as settled). Two real findings:
1. **`routes.js`'s Payscribe `TODO (Task 6): find + verify Payscribe's
   signature scheme...`** — already correctly tracked (Task 6, on hold
   pending PENDING_DOCS). No action needed.
2. **`providers/juicyway.js`'s `⚠️ Verify exact endpoint path in
   Juicyway docs` comment** — genuinely unresolved AND untracked. It's
   mentioned in the "Confirmed research findings" section (added by
   Task 5, which was webhooks-only in scope) but no task in the queue
   ever picked it up, unlike the equivalent gap for Korapay (closed by
   Task 7) and Paystack (queued as Task 8). This is exactly the kind of
   gap this task exists to catch. **Added Task 8b** above (same shape
   as Task 7/8, marked on hold under "Current focus: Korapay only" for
   the same reason Task 8 is) to close it — the comment itself is left
   in place in the code as the pointer to that task, same as how
   Task 6's Payscribe TODO comment still sits in `routes.js`.
Also found the "Known issues" bullet about provider error messages
reaching the client verbatim was now stale — Task 13's error-handling
pass (this same session, immediately prior) had already addressed it.
Updated that bullet to reflect what Task 13 actually did, and split
"No rate limiting anywhere" out as its own still-open line (Task 13
deliberately did not touch rate limiting — see that task's note).
Confirmed the "Confirmed research findings" section itself has no
remaining "secondary source, not yet confirmed" language attached to
anything actually shipped in code — the only genuinely open item
findings-side is the JuicyWay endpoint path above, now tracked as
Task 8b.
**Box checked because this task's own bar is "resolved or tracked",
not "everything is finished":** the audit's job was to catch anything
left dangling and make sure it has a home in the queue, which is now
true for both TODO-style comments found. Task 8b itself remains open
(on hold, same as Task 8) — that's expected follow-up work, not a
reason to leave this audit task unchecked. A future session revisiting
this audit should start by confirming Task 8b's status before
re-scanning from scratch.

---

## Cross-repo continuation

**Important — all three repos in this project (B-Pay-backend,
Mavins-web, and Velune) each have their own `handover.md`.** See "This
is a 3-repo project" near the top of this file for the full mechanics
(which commands change, how the hand-off to the human differs per
repo, the "Sibling repos" list with each repo's URL and push
mechanics) — summary: when a task below says to clone a different
repo, that session's job is to: (a) do the specific fix described,
fully inside that other repo's own clone (own commits, own patches,
own `git am`/push form — not this repo's), AND (b) update *that other
repo's* `handover.md` with what was done and what's left — so the next
session picks up the thread there, in that repo, using that repo's own
patch numbering and its own hand-off instructions. Don't let context
about a still-open B-Pay-backend task get lost just because work moved
to another repo — leave a one-line pointer back here if a Mavins-web or
Velune task turns out to depend on something not yet finished in this
file.

### Task 16 — Clone Mavins-web, diagnose the Korapay amount bug [x]
Clone `github.com/Zapier-codes/Mavins-web`. The reported symptom: "the
amount passed to Kora is not the correct amount." The most likely
cause, worth checking first: Mavins-web's pricing math
(`calculatePricing()` in `src/lib/campaign/pricing.ts`) works in
**cents** (`totalCostCents`, per `formatCents()` calls seen throughout
that codebase) — if the payment-initialize call site forwards
`totalCostCents` straight through as `amount` to this backend, and (per
Task 7's finding above) Korapay's `charges/initialize` wants the
**base currency unit** not subunits, the amount reaching Kora's
checkout could be 100x too large. **This is a hypothesis to verify
against the actual current code, not an assumption to act on
directly** — read `src/app/api/payments/initialize/route.ts` (or
wherever the current call site is; it may have moved) and trace the
exact value being sent before changing anything. Fix whatever the real
mismatch turns out to be. This task is diagnosis + fix; if the root
cause is more involved than a single unit-conversion bug, split further
into its own follow-up task in Mavins-web's `handover.md` rather than
trying to finish everything in one session.

**What was found / what changed:** Hypothesis confirmed, plus an
additional compounding bug the hypothesis didn't anticipate — full
detail lives in **Mavins-web's own `handover.md`, Task 26** (per this
file's own cross-repo continuation convention), not duplicated here.
Short version: `fund-wallet/page.tsx` did have the guessed 100x
unit-conversion bug, AND separately hardcoded `currency: 'NGN'` while
the amount itself was actually always USD — so it wasn't just scaled
wrong, it was tagged with the wrong currency too. Mid-session, the
project owner corrected the initial fix attempt (which had removed the
100x but kept NGN as the default): this app's real default/base
currency is USD, not NGN, and no client-side currency conversion
should happen at all — Korapay's own **Dynamic Currency Conversion
(DCC)** (confirmed against
developers.korapay.com/docs/dynamic-currency-conversion) is meant to
handle showing a non-US payer their own local currency at checkout,
driven by `payment_currency`/`settlement_currency` fields on the
charge request, converted at Korapay's live rate on Korapay's side.
**This repo's own code needed a companion change** to make that
possible: `routes.js`'s `POST /pay` previously destructured a fixed
field whitelist from `req.body` that silently dropped `payment_currency`/
`settlement_currency` even if a caller sent them — added both to the
destructure and to `paymentData`, and `providers/korapay.js` now
attaches them to the Korapay API payload when both are present.
Real, code-unverifiable prerequisite (documented in Mavins-web's Task
26, repeated here since it affects this repo's own Korapay integration
too): DCC requires the merchant's Korapay account to have Currency
Conversion product access (Kora-granted) and a per-currency dashboard
toggle enabled — neither can be confirmed or set from either repo's
code, and DCC requests will fail on Korapay's side until both are done
regardless of how correct this code is. Ties into this repo's own
Task 14 (blocked on real sandbox keys) for actually exercising this
end-to-end. Verified: `node --check routes.js` and `node --check
providers/korapay.js` both pass.

**Companion change, same pattern, different task (added later session):
Mavins-web's Task 30 ("Route currency + payment method by geo")
needed this repo to forward Korapay's `channels`/`default_channel`
checkout params the same way `payment_currency`/`settlement_currency`
already were above.** `routes.js`'s `POST /pay` destructure and
`paymentData` object now also include `channels`/`default_channel`
from `req.body`, forwarded unchanged. `providers/korapay.js`'s
`processPayment` now attaches `payload.channels`/`payload.default_channel`
to the actual Korapay API call, but only when `data.channels` is a
non-empty array — `default_channel` is dropped if `channels` wasn't
also supplied, matching Korapay's own docs ("the default channel must
also be specified in the channels parameter"). Confirmed directly
against developers.korapay.com/docs/checkout-redirect's own parameter
table for the four valid channel string values (`bank_transfer`,
`card`, `pay_with_bank`, `mobile_money`) — the actual country→channel
routing logic itself lives in Mavins-web's `korapayChannels.ts`, not
here; this repo's job is only to forward whatever the caller sends,
same "don't guess, let the caller/provider decide" principle as the
DCC fields. Verified: `node --check routes.js` and
`node --check providers/korapay.js` both pass; a throwaway `node -e`
script (deleted after) exercised the forwarding logic against five
cases — channels+default present, channels-only, default-without-
channels (correctly dropped), neither present, and an empty channels
array (correctly treated as absent) — all five correct. See
Mavins-web's own `handover.md`, Task 30, for the frontend routing
logic and the full write-up of why South Africa (ZA/EFT) was
deliberately left unmapped rather than guessed.

> **Tasks 17–24 below are historical.** They've been migrated into
> Mavins-web's own `handover.md` as Tasks 28–33 (see the box at the
> very top of this file). Kept here only for the original context/
> reasoning that produced them — don't work from these copies, and
> don't re-migrate them again.

### Task 17 — Mavins-web: skip fund-wallet/email step for already-authenticated users [x]
The guest-checkout flow (guest pays without an account → account
auto-created → session issued, designed earlier in this project) is
for people who don't have an account yet, so it collects email as part
of payment. For a user who's **already logged in**, hitting
insufficient funds should skip straight to the payment provider's
checkout using the account's already-known email — not re-show the
"fund your wallet, enter your email" guest flow. Find where the
insufficient-funds → fund-wallet routing decision is made, branch it
on auth state, and route logged-in users directly to checkout
initialization instead.

**Done, in Mavins-web (not this repo) — see that repo's own
`handover.md` → Task 28** for the full write-up (this task's own text
above was copied there verbatim as required by the "3-repo project"
convention, since it only lived here before).

**Correction (this note originally pointed at the wrong task number
and a nonexistent commit hash — a stale patch landed before a
corrected one was ready; fixed here, see this file's own patch log for
how that happened):** the real task landed as Mavins-web's own
**Task 28**, not Task 26 — a different, unrelated task (a Korapay
currency/unit fix) had already claimed "Task 26" there by the time
this one shipped. Verified against the actual repo (not the
handover.md's own self-reported hash, which doesn't exist there —
likely a pre-`git am` local hash that changed once applied): the real
commit is `1ae8ceb`.

**What actually shipped is also more refined than this task's original
framing ("branch on auth state") above** — corrected mid-session by
Mavins-web's own session, per an explicit product-owner clarification:
the axis that matters isn't authenticated-vs-guest, it's whether the
account has ever held funds. A brand-new authenticated user has a
wallet balance of exactly 0 and provably has nothing to check, so
`promote/page.tsx` now sends them straight to checkout with no wasted
`createCampaign` attempt; a *returning* user with a real (if possibly
insufficient) balance still attempts `createCampaign` first, falling
back to checkout only on an actual insufficient-funds error. Guests
are unaffected — still routed through `/fund-wallet` since they have
no known email to skip collecting. `npx tsc --noEmit` clean per that
repo's own note. Live end-to-end check still recommended post-deploy
(no sandbox network access to Supabase/Korapay from either repo).

### Task 18 — Mavins-web: reconcile the real country/currency list [ ]
`TARGET_COUNTRIES` (`src/lib/campaign/geoAffinity.ts`) and
`COUNTRY_CURRENCY` (`src/app/promote/page.tsx`) were 14 and 20 entries
respectively as of the last session touching this repo — they should
probably be the same list, or one should clearly be a superset with a
documented reason why. Reconcile them into a single source of truth
(however many countries/currencies that turns out to be — don't target
a specific number), and make sure whatever B-Pay-backend's Task 9
currency table ends up covering actually matches this list exactly on
both sides. This task may need its own follow-up in B-Pay-backend's
`handover.md` if the two repos' currency lists don't line up once this
is done — leave that note there if so.

**Status check this session (from B-Pay-backend, while starting Task
9b below) — this task has NOT been done, and this file's own note
about it was stale/misleading:** cloned Mavins-web fresh and confirmed
directly (not from memory) that `TARGET_COUNTRIES` grew from 14 to 25
entries via *Mavins-web's own* Task 23 ("shuffle 8-of-25 countries by
genre"), but that was a country-*targeting-pool* task, not a currency
reconciliation task — it never touched `COUNTRY_CURRENCY`, which is
still the original 20-entry list. Comparing the two lists directly as
they stand now: only 12 country codes appear in both (NG, US, GB, GH,
KE, ZA, CA, AU, IN, AE, BR, MX). `TARGET_COUNTRIES` has 13 codes
`COUNTRY_CURRENCY` doesn't (FR, DE, JM, NL, CI, SN, TZ, UG, EG, ES, IT,
SE, KR), and `COUNTRY_CURRENCY` has 9 codes not in `TARGET_COUNTRIES`
at all (EU, PK, BD, ID, PH, MY, SG, SA, TR — these look like a leftover
generic currency-conversion list, unrelated to campaign targeting). The
two lists are further apart in absolute terms than when this task was
first written (14-vs-20 has become 25-vs-20, with less overlap
proportionally). **Also: this task, as written here in B-Pay-backend's
`handover.md`, was never actually copied into Mavins-web's own
`handover.md` as a task in that file's queue** — confirmed by grepping
Mavins-web's `handover.md` for "TARGET_COUNTRIES"/"COUNTRY_CURRENCY"/
"reconcile"; the only hits are Task 23 (unrelated scope, above) and
nothing else. So there is currently no queued task anywhere that will
pick this up. **Whoever next works Mavins-web should add this as a real
task in that repo's own `handover.md`** (not just leave it living only
here) before attempting it, per this whole project's cross-repo
continuation pattern.

### Task 19 — Mavins-web: route currency + payment method by geo [ ]
Use the existing `detectUserGeo` service (via ipapi.co, already present
in this codebase per an earlier session) to determine the user's
country, then: for African countries where Korapay supports
mobile-money/bank-transfer (per B-Pay-backend's confirmed findings —
check that file's current state, it may have grown since this note was
written), route the checkout amount + currency + preferred method
accordingly; for countries where none of the backend's providers has
local rails, fall back to USD via whichever provider/channel supports
USD. This depends on Task 18's reconciled currency list (still not
done as of this file's latest check — see that entry) and on
B-Pay-backend's Task 10 (provider routing) being done first — check
both before starting.

### Task 20 — Mavins-web: no conversion/display for USD-default users [ ]
If the detected/selected currency is USD, don't show a converted
"local" amount anywhere (the app's own internal default is already
USD, so there's nothing to convert *from* for these users) — audit
wherever the "≈ local currency" display was added (e.g. the pricing
card's `localCurrency` prop, from earlier promote-page work) and make
sure it's conditionally skipped, not just showing "≈ $X USD" redundant
with the primary total.

### Task 21 — Mavins-web: update this repo's own handover.md [ ]
Once Tasks 16–20 (or however many of them got done) are complete,
update Mavins-web's own `handover.md` with what happened, any newly
discovered follow-up tasks, and continue that file's own existing task
queue (it already had unfinished tasks — Task 6 onward — before this
payment work started; don't lose track of those). This is the
carry-over step described at the top of this section.

### Task 22 — Clone Velune, investigate campaign placement display [x]

**Status corrected this session — this had already happened, just on
a track that never reported back here.** Cloned `Velune` directly and
confirmed: it already has a built "Campaign Card" feature, documented
in that repo's own `HANDOVER_CAMPAIGN.md` (separate from `HANDOVER.md`,
which covers an unrelated EQ/DSP subsystem in the same Android app).
**Note the description mismatch, for whoever reads this next:** this
task as originally written assumed an existing display that "isn't
wired correctly" — what's actually in `HANDOVER_CAMPAIGN.md` reads as
a fresh, deliberate, ethically-reviewed build (v1, "started and mostly
built in one session"), not a fix to something broken. Possibly the
original framing was based on an earlier, since-superseded state, or
on the unrelated `phoenix-boss/Mavins` repo (see below) rather than
Velune itself — not fully resolved, but the investigation this task
asked for is done either way, and Velune's own file now has its own
real open items (see its "8. Not done / open" — no live Supabase
credentials wired in is the current blocker there).

**Separately worth flagging:** Velune's `HANDOVER_CAMPAIGN.md`
references a **fourth repo**, `github.com/phoenix-boss/Mavins`
(Expo/React Native, `expo-video` branch) — its
`hooks/useQuickPicks.ts`/`CampaignManager` fabricates listener/
geography/device numbers via a seeded PRNG and writes them into a real
`play_count` column, permanently mixing fake and real data. The
Velune session that found this **declined to port it**, on the
project owner's own accepted correction. This isn't part of the
current 3-repo scope's active work, but it's real, load-bearing
context the project owner should be aware of if that repo comes up
again.

### Task 23 — Confirm this backend no longer needs to be the reference source, and that its real caller is the edge function [ ]
Per "Project owner decisions" → Decision 1 (as corrected): the app
generates and owns the payment `reference` client-side and writes it to
Supabase, but this backend's actual caller is the **Supabase Edge
Function**, not the app directly — the edge function calls `POST /pay`
with that reference, and also owns webhook reconciliation. Audit
`POST /pay` in `routes.js` and confirm that path: (a) still works
correctly when the caller (the edge function) always supplies its own
`reference` (the common case going forward), and (b) decide whether
`generateReference()`'s own-reference fallback should stay as a defensive
default for malformed/legacy callers or be treated as a bug signal (log a
warning) now that it's not supposed to be relied on. Also worth checking
as part of this audit: whether `POST /pay` needs any caller-identity/auth
check now that its intended caller is a trusted Supabase Edge Function
rather than an untrusted client directly (this backend currently has no
such check — flag it as a new "Known issues" bullet if it's genuinely
missing, don't fix it in this same task unless it's trivial). Don't
remove the reference fallback outright without checking whether any
current caller still depends on it — this is an audit-and-decide task,
not an automatic deletion.

### Task 24 — Mavins-web: implement wallet-crediting + first-time-vs-returning-user logic [ ]
Per "Project owner decisions" → Decisions 2 and 3 above (owner-provided,
recorded in this file for continuity — implementation belongs in
Mavins-web, not here). Copy Decisions 1 (as corrected), 2, and 3 into
Mavins-web's own `handover.md` as their own task(s) before starting: (1)
client-side reference generation + Supabase write, **and** the Supabase
Edge Function that calls this backend's `POST /pay` with that reference
(the app itself should stop calling this backend directly, if it
currently does) — this unblocks this repo's Task 23; (2) wallet-balance
computation on confirmed webhook
(full amount minus platform fee, credited only for returning users doing
a top-up — first-time users who pay directly for a campaign see no
wallet balance change, ever); (3) the shared user/admin success screen
with the animated country-interconnection pipeline visualization
(central hub node, animated links out to each selected country) shown on
confirmed payment. Split further once in Mavins-web's own file if any of
(1)/(2)/(3) turns out to be bigger than one session — same one-task-per-
session rule as this file.

### Task 25 — Mavins-web: ipapi.co geo-detection at app initialization, global + persistent-through-login, NOT stored in Supabase [x]
**Project owner instruction, recorded here verbatim in spirit before
implementation:** IP geolocation (via ipapi.co) should be detected
**once, at app initialization — i.e. on the user's first visit/page
load, before or independent of any auth state** — made available
**globally** across the app (every component/page that needs currency,
country, or payment-routing context reads the same detected value, not
a fresh per-component fetch), and that detected value must **persist
through login** — logging in must never reset, override, or re-trigger
the geo detection. **Explicitly do NOT persist this to Supabase or any
other server-side/database store tied to the user's account.** The
stated reason is important context for *how* to build this, not just
*that* to build it: the project owner wants to **welcome users on a
VPN** — if geo were written to a user's Supabase row, a returning VPN
user's exit-node location could get silently overridden by (or conflict
with) a previously-stored "real" location, or a session could end up
trusting stale account data over what the person's connection is doing
*right now*. The fix for that isn't "detect VPN and block it" (not
asked for, don't add it) — it's simply: **never let this be anything
other than fresh, client-side, per-visit, in-memory-or-session-scoped
state.** A VPN user should be treated exactly like anyone else browsing
from wherever their connection currently appears to be.

**"Do it professionally like industry standards" — concrete shape this
implies, not just a general instruction to be careful:**
- A dedicated React Context/provider (e.g. `GeoProvider`, mounted in
  `src/app/providers.tsx` **as a sibling to `AuthProvider` and
  `ThemeProvider`, not nested inside or dependent on either** — this is
  what actually guarantees "persists through login": if it's not a
  child of `AuthProvider` and doesn't read `user`/session state at all,
  logging in has structurally no way to reset it), fetching once on
  mount and exposing `{ country, currency, loading, error }` (or
  similar) via a `useGeo()` hook, the same pattern
  `ThemeProvider`/`useTheme()` already establishes in this codebase —
  match that existing convention rather than inventing a new one.
- **Check whether `detectUserGeo` (referenced in this file's own Task
  19 above, "via ipapi.co, already present in this codebase per an
  earlier session") already does the fetch correctly** — if so, this
  task may mostly be *relocating* an existing call up to true app-root
  initialization and wrapping it in a proper global provider, not
  writing a new ipapi.co integration from scratch. Read the current
  code before assuming either way.
- **Don't block initial render on the fetch.** Expose a sensible
  loading state and a safe default (e.g. `currency: 'USD'` — matches
  this project's own established "USD is the app's default, only
  convert away from it when we know better" principle from Mavins-web's
  Task 20) while the request is in flight, rather than a blank screen
  or a layout shift once it resolves.
- **Graceful failure is required, not optional.** ipapi.co's free tier
  is rate-limited (historically ~1,000 requests/day on HTTPS) and can
  fail or throttle — geo detection is an enhancement to currency/payment
  routing, not a critical-path dependency the app should break over. On
  failure, fall back to the same USD default as the loading state, log
  the failure, and let the user continue normally (this app already
  supports a manual/explicit currency choice in the payment flow per
  earlier work — confirm that still works as an override regardless of
  what geo-detection returns or fails to return).
- **"Persist" almost certainly means "for this visit/tab session," not
  "forever across devices"** — re-reconcile this with the project owner
  directly if genuinely ambiguous when this task is picked up, but the
  default interpretation given everything above (fresh-per-visit, VPN-
  friendly) should be: in-memory React state for the life of the page
  load is the right baseline. `sessionStorage` (not `localStorage`) is
  a reasonable enhancement to survive an in-tab reload without a second
  ipapi.co call burning rate-limit budget — but per the "no Supabase"
  instruction's actual reasoning above, do not reach for `localStorage`
  either, since that would persist a stale location across visits/days
  in the same way a Supabase-backed store would, defeating the same
  VPN-friendliness goal for a returning user whose location has since
  changed (e.g. connected to a different VPN exit node, or genuinely
  traveled).
- Confirm no other part of the codebase (e.g. wherever Task 19's
  geo-based currency/method routing landed, once that task is done) ends
  up making its *own* separate `detectUserGeo`/ipapi.co call instead of
  reading from this new global context — that would silently defeat the
  "one fetch, globally shared" goal even if this task's own code is
  otherwise correct.

This is a Mavins-web-only task — no B-Pay-backend code changes. Recorded
here per this project's cross-repo continuation convention; the session
that picks this up should clone Mavins-web, read *that* repo's own
`handover.md` in full first (it may have already grown a related task,
or partially done this — don't duplicate), do the work there, and update
Mavins-web's own file per its own process, same as Tasks 17–24 above.

**Done — confirmed directly against Mavins-web, not from a stale
note:** implemented as `GeoProvider` (`src/components/providers/
GeoProvider.tsx`), mounted outside/alongside `AuthProvider` exactly as
specified above. Storage ended up as `localStorage` with a 24h TTL
rather than `sessionStorage` — a deliberate choice by that session, not
an oversight: still 100% browser-local (never touches Supabase, same
no-account-tagging goal this task cared about), but survives a closed
tab too, with the TTL specifically so a VPN toggle or genuine location
change gets re-detected within a day rather than staying wrong until
the tab closes. See Mavins-web's own `handover.md` → Task 27 for the
full write-up. Commit `5c1b4d2` on Mavins-web's `main`.

---

### Task 41 — Central Korapay webhook gateway for multi-tenant apps [x]

**Migrated from Mavins-web's own `handover.md` (that repo's Task 41 —
same number, kept identical on purpose since this is one task tracked
in two places by necessity: decisions recorded there, build here).**
Korapay's dashboard has exactly one webhook-URL slot, account-wide.
The product owner is building multiple other multi-tenant apps beyond
Mavins-web, all needing Korapay webhook events — every app registering
its own URL directly isn't possible, so this backend (already holding
the Korapay credentials) becomes the one thing Korapay's dashboard
points at, fanning events out to whichever app actually owns each one.

**Decisions already confirmed by the product owner (recorded in
Mavins-web's own file, copied here for this repo's own record):**
Option A — this backend is the gateway, not a separate repo. This
app's own reference prefix (the only tenant so far): `MAVW-`.

**Built this session — `webhookGateway.js` (new file):**
- **Routing table**, env-var driven: `TENANT_ROUTES` maps a reference
  prefix (first segment before `-`, uppercased) to a downstream app's
  forward URL + its own internal forwarding secret. One entry today
  (`MAVW` → `MAVW_WEBHOOK_URL` / `MAVW_WEBHOOK_FORWARD_SECRET`, both
  new env vars — **not yet set in Render's dashboard, that's a manual
  step for the product owner**, added to `render.yaml` as
  `sync: false` placeholders same as the existing provider keys).
  Adding a new tenant later is config-only, no code change.
- **Korapay's own signature verified exactly once**, unchanged from
  the existing code (`providers/korapay.js#verifyWebhookSignature`) —
  this now lives in the gateway path only; downstream apps never see a
  raw Korapay signature at all, they verify the gateway's own internal
  one instead (next point).
- **Internal forwarding signature** — HMAC-SHA256 over the forwarded
  JSON body, using each tenant's own `forwardSecret` (never Korapay's
  own secret), sent as `X-Gateway-Signature`. Verified in isolation
  this session (correct-secret accepts, wrong-secret rejects, tampered
  payload rejects — all three checked directly, not assumed).
- **Idempotency** — dedupes on `` `${event}:${data.reference}` ``
  (Korapay's payload isn't confirmed to carry its own globally unique
  event id anywhere this codebase has seen; this is the documented
  fallback from this task's own spec, not a guess). A Korapay retry of
  an already-recorded event returns the existing record instead of
  forwarding a second time — confirmed directly via a duplicate-call
  test.
- **Persist-then-forward, in-memory by design — architecture decision
  now confirmed, this is not a gap:** the event store is **in-memory
  only** (a `Map`), not backed by any database. **Product owner has
  confirmed this backend gets no database, ever, structurally** — every
  app using this as its canonical payment gateway already has its own
  database; this backend's job stops at verify-once + forward, and
  durable recording is each app's own responsibility via its own edge
  function (see Mavins-web's Task 42: its `korapay-webhook` receives
  the forward and records into its own Supabase). This resolves the
  question Task 12 left open for this same fork — not "which DB", but
  "no DB here, period, by design." In-memory retry is durable for the
  life of the running process (a downstream app being briefly
  unreachable gets retried correctly within that window); an event
  lost to a restart before both this gateway's retry sweep and
  Korapay's own webhook-retry succeed is an accepted, deliberate
  tradeoff for staying stateless, not an oversight.
- **Retry sweep** — `index.js` now calls `retryFailedEvents()` every
  60s (same `setInterval` pattern as the existing outbound-IP monitor
  in that file), fixed backoff schedule (30s → 2min → 10min → 30min →
  1hr), gives up after 5 attempts and logs loudly once rather than
  retrying forever or spamming the log every sweep.
- **`GET /gateway-stats`** (new, unauthenticated, counts only — never
  raw event payloads, those can carry customer emails/amounts) for
  quick visibility into the gateway's current in-memory state.
- `webhookHandlers.korapay` in `routes.js` now calls
  `handleGatewayEvent(event, data)` after its existing signature check
  and event-type logging (both unchanged) — the response to Korapay
  (`{ received: true }`) happens regardless of the forward attempt's
  own outcome, so a slow/failing downstream tenant never holds up
  Korapay's own webhook delivery or risks Korapay's retry storm.

**Verified this session:** `node --check` on all three touched files;
a standalone functional smoke test of `webhookGateway.js` (unroutable
reference correctly rejected, routable reference attempts a forward
and records the failure with a correct attempt count, duplicate event
deduped with no second forward attempt, `getGatewayStats()` reports
correctly); a standalone signature test (correct secret verifies,
wrong secret rejects, tampered payload rejects). **Not verified: an
actual live forward to a real Mavins-web endpoint** — no such endpoint
exists yet, see the next paragraph.

**Real remaining work, not done here, each belongs somewhere else:**
1. ~~The product owner needs to actually set `MAVW_WEBHOOK_URL` and
   `MAVW_WEBHOOK_FORWARD_SECRET` in Render's dashboard~~ — **done.**
2. ~~Mavins-web's own follow-up (swap `korapay-webhook`'s
   verification)~~ — **done, see that repo's Task 42.**
3. ~~The Korapay dashboard webhook URL itself still needs
   re-pointing~~ — **done, but caught a real mistake along the way:**
   it was initially set to the **bare domain**
   (`https://b-pay-backend.onrender.com`) instead of the actual route
   (`/api/webhooks/korapay`) — this backend's root path only has a
   `GET` handler, so every webhook 404'd silently for a period before
   this was caught and corrected. See the correction note at the very
   top of this file (START HERE box) for the full account — flagging
   here too since this is exactly the kind of detail a future session
   skimming past this checklist could otherwise miss. **Still not
   independently confirmed:** a real event actually landing in
   `/gateway-stats` since the correction — check that before assuming
   this is truly resolved, don't just trust this checklist.

(Point 4, the persistence-durability question, is resolved — see
the "Persist-then-forward" bullet above. No database, ever, by design;
nothing further to decide there.)

---



- `0001-webhook-route-skeleton.patch` — Task 2 (webhook routing
  skeleton, `routes.js`). Verified to apply cleanly with `git am`
  against `3811f7f` (Task 1's commit) and pass `node --check` on both
  touched/importing files, in a fresh `/tmp` clone, before handing
  off.
- `0002-paystack-webhook-signature.patch` — Task 3 (Paystack webhook
  signature verification + `charge.success` handling,
  `providers/paystack.js` + `routes.js`). Verified to apply cleanly
  with `git am` against `9fc20f7` (the pushed, real hash of Task 2's
  commit on `origin/main` — not the local `567d518` it had before the
  human pushed it) and pass `node --check` on both touched files, in a
  fresh `/tmp` clone, before handing off.
- `0003-korapay-webhook-signature.patch` — Task 4 (Korapay webhook
  signature verification + 6-event handling,
  `providers/korapay.js` + `routes.js`). Verified to apply cleanly
  with `git am` against `1d7fbd3` (the pushed hash of Task 3's commit
  on `origin/main`) and pass `node --check` on both touched files, in
  a fresh `/tmp` clone, before handing off.
- `0004-korapay-paystack-currency-routing-validation.patch` — Task 10
  (partial, Korapay-focus session; currency-aware validation for
  Paystack/Korapay in `POST /pay`, `utils/helpers.js` +
  `routes.js`). Verified to apply cleanly with `git am` against
  `1fe8a34` (Task 9's commit, the local HEAD this session started
  from — this session did not yet know that commit's real pushed hash
  on `origin/main`, since it hadn't been pushed yet when this session
  ran; whoever runs `git am` for this patch should confirm
  `git log -1` shows `1fe8a34` as the current HEAD before applying,
  and if not, note the actual hash here) and pass `node --check` on
  both touched files, in a fresh `/tmp` clone, before handing off.
- `0005-post-pay-request-validation.patch` — Task 11 (request-shape
  validation on `POST /pay`: `utils/helpers.js` + `routes.js`). Same
  caveat as `0004` above: verified with `git am` against `192fe24`
  (this session's own prior Task 10 commit, not yet known to be pushed
  to `origin/main` at the time this patch was generated) in a fresh
  `/tmp` clone, `node --check` passing on both touched files. If
  `0004` has already been applied and pushed by the time this patch is
  applied, `192fe24` should already be the current `origin/main` HEAD
  and this should apply with no extra steps; if not, apply `0004`
  first.
- `0006-reference-format-validation.patch` — Task 12 (in-scope half:
  client-supplied `reference` format validation, `routes.js` only).
  Same caveat as `0004`/`0005`: verified with `git am` against `0fd6260`
  (this session's own prior Task 11 commit) in a fresh `/tmp` clone,
  `node --check` passing. Apply `0004` and `0005` first if they
  haven't been pushed yet.
- `0007-handover-owner-decisions-wallet-reference.patch` — docs-only,
  not tied to a numbered task box: records the project owner's
  Decision 1/2/3 (reference storage, wallet crediting, success UI),
  adds Task 23/24. Verified with `git am` against `2423c7c` (Task 12's
  commit) in a fresh `/tmp` clone. **Superseded in part by `0008`
  below — apply both, in order, `0007` then `0008`.**
- `0008-handover-decision1-correction-edge-function.patch` — docs-only
  correction to `0007`: Decision 1 originally said the app calls this
  backend directly; the project owner corrected this — the app writes
  the reference to Supabase, but the **Supabase Edge Function** is
  this backend's actual caller, not the app. Updated Decision 1, the
  matching "Known issues" bullet, and Tasks 23/24 accordingly. Verified
  with `git am` against `5582fdf` (this session's own prior commit,
  i.e. `0007` applied) in a fresh `/tmp` clone. Requires `0007` applied
  first — will not apply standalone against `2423c7c`.
- `0009-task9b-dependency-check-mavins-web-task18-fix.patch` —
  docs-only, not tied to a numbered code task: attempted Task 9b (next
  unchecked task in queue order), found it's still blocked on
  Mavins-web's currency-list reconciliation, and found this file's own
  reference to that dependency ("Mavins-web's Task 18") was stale/
  misleading — Mavins-web's *own* Task 18 is an unrelated task, and the
  reconciliation was never actually added to that repo's own queue.
  Corrected Task 9b, Task 18, and Task 19's cross-references
  accordingly. Verified with `git am` against `bfb4905` (this session's
  own prior commit, i.e. `0007`+`0008` applied) in a fresh `/tmp` clone.
  Requires `0007` and `0008` applied first.
- `0010-three-repo-navigation-sibling-repos.patch` — docs-only, not
  tied to a numbered code task: added the "This is a 3-repo project"
  section near the top of this file plus a "Sibling repos" block,
  documenting the exact command/hand-off changes needed when a task's
  real subject is Mavins-web or Velune rather than this repo. Confirmed
  via GitHub API that Mavins-web is not a fork (unlike this repo), so
  it doesn't share this repo's fork→PR mechanics. Cross-linked from
  step 10 and from "Cross-repo continuation". Verified with `git am`
  against `a08fe58` (this session's own prior commit, i.e.
  `0007`+`0008`+`0009` applied) in a fresh `/tmp` clone. Requires
  `0007`, `0008`, and `0009` applied first.
- `0011-mavins-web-push-step-and-folder-casing-fix.patch` — docs-only
  correction to `0010`: Mavins-web's push step was missing from the
  "Sibling repos" note (it does push directly to `main`, no PR, since
  it isn't a fork), and the local clone folder is `mavins-web`
  (lowercase), not the GitHub repo's own `Mavins-web` casing. A
  matching correction was made in Mavins-web's own `handover.md`
  (separate patch, that repo — see its own patch log). Verified with
  `git am` against `41a8b4c` (this session's own prior commit, i.e.
  `0007`–`0010` applied) in a fresh `/tmp` clone. Requires `0007`
  through `0010` applied first.
- `0012-task12-checkbox-reconciliation.patch` — docs-only, closes out
  Task 12: no source file needed changing — `routes.js` already had
  `assertValidReferenceFormat` wired into `POST /pay` exactly as
  Task 12's own note described, and the storage/architecture decision
  it was waiting on had already arrived (Decision 1). The task's own
  "why box stays unchecked" paragraph had gone stale once the very next
  paragraph recorded that decision, leaving two contradictory notes
  back to back. Checked Task 12's box and replaced the stale paragraph
  with a session note explaining the reconciliation. Verified with
  `git am` against `71a9a3b` (this session's own prior HEAD, i.e.
  `0007`–`0011` applied) in a fresh `/tmp` clone. Requires `0007`
  through `0011` applied first.
- `0013-error-message-sanitization.patch` — Task 13 (error-handling-
  review half only; rate limiting explicitly descoped this session per
  project owner instruction — see the task's own note). Added
  `providerError()` + the `isProviderMessage` flag and `isConfigError`
  tagging in `utils/helpers.js`; used `providerError()` at all 7
  provider-response-failure throw sites across `providers/paystack.js`,
  `providers/korapay.js`, `providers/juicyway.js`, and
  `providers/payscribe.js`; added `clientSafeMessage()` in `routes.js`,
  used by all three catch blocks (`POST /pay`, `GET /verify`,
  `POST /webhooks/:provider`) — the last of which also picked up an
  incidental fix (now respects `error.statusCode` instead of always
  answering 500, matching `POST /pay`/`POST /webhooks/:provider`).
  Verified with `git am` against `de007f9` (this session's own prior
  commit, i.e. `0012` applied) in a fresh `/tmp` clone, `node --check`
  passing on all six touched files. Requires `0007` through `0012`
  applied first.
- `0014-task15-audit-pass.patch` — docs-only, Task 15 (final audit
  pass). Found `providers/juicyway.js`'s endpoint-path `⚠️` comment was
  genuinely unresolved and had never been turned into a queued task
  (unlike the equivalent Korapay/Paystack gaps, closed by Task 7 /
  tracked as Task 8) — added Task 8b to close that gap, on hold under
  "Current focus: Korapay only" same as Task 8. Also refreshed the
  "Known issues" bullet about provider error messages, which had gone
  stale now that Task 13 (this same session, immediately prior)
  addressed it. Checked Task 15's own box — its bar is "resolved or
  tracked," which is now true for both TODO-style comments found; the
  new Task 8b remains separately open. Verified with `git am` against
  `2eeb4e3` (this session's own prior commit, i.e. `0013` applied) in a
  fresh `/tmp` clone. Requires `0007` through `0013` applied first.
- `0015-task17-checkbox-crossref.patch` — docs-only, checks off Task 17
  in this file (implementation itself lives in `mavins-web`, not this
  repo). **This version's Task 26 / `be3ee34` reference was wrong (see
  the `b-pay-backend-task17-correction.patch` entry directly below for
  why and how it was fixed) — landed on `main` as commit `f7df7f0`
  anyway**, ahead of the corrected version being handed over, because
  the human had already downloaded this file before the correction was
  ready. Verified with `git am` against `5c51467` in a fresh `/tmp`
  clone at the time — the patch itself applied cleanly; the *content*
  was stale, not the mechanics.
- `b-pay-backend-task17-correction.patch` — docs-only follow-up to the
  above, landed on top of `f7df7f0` (not a rewrite of it — already
  pushed/public, so corrected forward instead of amended). Fixes Task
  17's note and the patch-log entry above: the real cross-repo task is
  Mavins-web's own **Task 28** (not Task 26 — that number was claimed
  by an unrelated Korapay currency fix there by the time this landed),
  real commit `1ae8ceb` (not `be3ee34`, which was a local hash from a
  patch that was itself later discarded — see Mavins-web's own note on
  this task for the full story: a parallel session had already
  implemented this same task there, more thoroughly, per a mid-session
  product-owner correction to route by wallet balance rather than auth
  state alone). Verified with `git am` against a fresh clone of this
  repo's actual current `origin/main` (`f7df7f0` at the time). Uses
  this project's newer `<repo-slug>-<description>.patch` filename
  convention (see "Unified hand-off command format" above) rather than
  a new sequential number — from here on, prefer that convention for
  new patches in this file too, so numbering doesn't have to track
  three repos' independent, interleaved sessions.


---

## Task 42 — CRITICAL: POST /payout had zero authentication — Part A fixed; Part B split, payload-shape bug found AND fixed, response-parsing corrected; `/pay` auth-extension implemented on this side (Part c-a), cross-repo c-b urgent-not-done; `/verify`/`/banks` (Part b-b) still open [x] (Part A + Part B's amount-verify + payload-shape + response-parsing + b-a's facts-and-verdict + c-a)

**Found by a Mavins-web session, flagged there instead of here (the
wrong repo — the vulnerable code lives in this one), confirmed
directly against this repo's own `routes.js` before writing anything
down, not taken on trust.** `POST /payout` — the route that actually
moves real money out via Korapay's disburse API — had **no
authentication of any kind**: no API key, no shared secret, no IP
allowlist, nothing. Any request from anywhere on the internet, with no
credentials at all, could trigger a real payout to an arbitrary bank
account by supplying `amount`/`bank_code`/`account_number` directly.
This is the single most severe finding in this project's history —
prioritized over the literal next item in the feature queue given the
live financial risk, same judgment call this project's own "urgent
security finding" precedent elsewhere would support.

**Split into Part A/B, this session, per the standing mandatory
task-splitting rule — Part A only, built and verified:**

### Part A — authentication on `/payout` specifically [x]
New `requireInternalApiKey` middleware (`utils/helpers.js`) — a shared
secret (`INTERNAL_API_KEY`, new env var, `render.yaml` updated,
**not yet set to a real value in Render's dashboard** — manual
product-owner step, same class of action as every other secret in
this file), sent by trusted callers as `X-Internal-Api-Key`, compared
with `crypto.timingSafeEqual` (same rigor already established in
`webhookGateway.js`'s own signature checks — deliberately not a plain
`===`, which would leak timing information). **Fails closed if the
env var itself is unset** — same posture already used everywhere else
in this codebase a secret might be missing; an unconfigured key must
never be silently treated as "no auth required." Applied to `POST
/payout` only, via `router.post('/payout', requireInternalApiKey,
async (req, res) => { ... })` — a single middleware argument, minimal
surface change.

**Verified, not assumed:** `node --check` on both touched files; a
standalone functional test against 6 cases, all passing — correct key
lets the request through, missing key rejected (401), wrong key
rejected (401), a key of different byte length rejected without
crashing (401 — confirms the length-check-before-`timingSafeEqual`
guard works, since that function throws on mismatched lengths rather
than returning false), empty-string key rejected (401), and critically
**the env var being unset fails closed with a 500, never lets a
request through** — the one case that would have been catastrophic to
get wrong.

### Part B — split into a/b/c this session, per the standing mandatory task-splitting rule [ ] (a only)

Originally two flagged items; split into three parts along their
actual dependency lines rather than the original 1/2 grouping — item 2
(amount-unit verification) is fully independent and became Part a;
item 1 (extend auth to other routes) splits into an investigation
(Part b: is it even appropriate) and its own implementation (Part c),
since "check first, then maybe build" was always two different jobs
bundled into one bullet.

### Part a — independently verify `processPayout`'s amount-unit convention against Korapay's real payout docs [x] (documentation only, no code changed)

**Done this session (2026-09-01) — the narrow question is answered,
but a much bigger, previously-undocumented problem surfaced while
answering it.**

**The amount-unit question itself: confirmed correct, no change
needed.** Fetched `developers.korapay.com/docs/payout-via-api`
directly (not relied on from memory or the collection-side citation) —
its field reference describes `destination.amount` as the transaction
amount "in two decimal places," i.e. base currency units (e.g.
`1500.00` for fifteen hundred naira), the same convention already
confirmed for the collection side. This matches the existing
`getAmountFormat('korapay', ...)` config exactly
(`{ unit: 'base', multiplier: 1 }`) — Part A's assumption that the
payout side shares the collection side's convention turns out to be
right, now independently confirmed rather than merely assumed.

**What actually needs fixing, found in the same pass — flagged, NOT
built, per explicit instruction to keep this session documentation
only:** `processPayout()`'s entire request payload shape doesn't match
the real, current Payout API at all. The official schema requires
every payout-specific field nested under a single `destination`
object, with `destination.type` (`bank_account` or `mobile_money`)
**required** — this codebase's payload is flat at the top level and
never sets a `type` field anywhere. Field-by-field, as currently sent
vs. what Korapay's docs actually require:

| Sent today (`providers/korapay.js`, top-level) | Required today (nested under `destination`) |
|---|---|
| *(nothing — `destination.type` never set)* | `destination.type` — **required**, `bank_account` or `mobile_money` |
| `amount` | `destination.amount` |
| `currency` | `destination.currency` |
| `bank_code` | `destination.bank_account.bank` |
| `account_number` | `destination.bank_account.account` |
| `narration` | `destination.narration` |
| `customer` (optional in this code) | `destination.customer.email` — **required** |
| `payment_method` | *(not a real field on this endpoint at all)* |

**Practical effect: every real payout call this code makes almost
certainly gets rejected outright by Korapay** — not a wrong-amount
bug, a wrong-shape bug, independent of and more severe than the
amount-unit question this part was actually scoped to check. This
should very likely be fast-tracked ahead of Part b/c given it affects
whether payouts function at all, not just their security — but that's
a product-owner call, not this session's to make unilaterally
(explicit instruction this session: documentation only, don't fix it
even though the severity is high).

**Smaller, secondary finding, also flagged rather than corrected
here:** the currently-confirmed Korapay currency list
(`CONFIRMED_PROVIDER_CURRENCIES.korapay` in `utils/helpers.js`)
includes `TZS`, but the *current* live payout-via-api docs page's
currency field lists only `NGN, KES, GHS, XOF, XAF, EGP, ZAR, USD` for
payouts — no TZS. Either the docs changed since that list was first
confirmed, or TZS payout support may not actually exist and the
original citation was mistaken. Not corrected here since
`CONFIRMED_PROVIDER_CURRENCIES` is a shared list other code paths
depend on (including the collection side, where TZS may well still be
correct) — a future session should re-verify TZS specifically for
payouts before either removing it or confirming it stays.

**One more real detail worth a future session knowing, not urgent
enough to block anything:** Korapay's docs state that XAF and XOF
payouts are only accepted in multiples of 5 or 10 — an amount like
XAF 101 must be rounded to 100 or 110 before the request, or it's
rejected. Nothing in this codebase currently handles that rounding
rule for any currency.

### Part a's own fix — the payload-shape bug it found, now fixed (2026-09-01) [x]

**This session, split from the original a/b/c grouping per direct
instruction ("split into a and b, do only a") — treated as its own
a/b, separate from the pre-existing Part b/c above (those are about
the unrelated auth-extension question).** Independently re-verified
the mismatch before fixing anything — fetched
`developers.korapay.com/docs/payout-via-api` again AND a community
Elixir client library's own published type spec
(`@type destination() :: %{type: String.t(), amount: float(),
currency: String.t(), narration: String.t(), bank_account:
short_bank_account(), customer: customer()}`) — **two independent
sources agreeing**, not relying on the prior session's own citation
alone.

**Fixed in `providers/korapay.js#processPayout()`:** the entire
outgoing payload now nests under `destination` exactly as both sources
describe — `type` (explicitly sent, `'mobile_money'` when
`data.payment_method === 'mobile_money'`, `'bank_account'` otherwise;
Korapay's own docs say this defaults to `bank_account` if omitted, but
there's no reason to lean on an undocumented-in-the-official-reference
default when the value is always known at call time), `amount`,
`currency`, `narration`, `bank_account: { bank, account }` (renamed
from the old flat `bank_code`/`account_number`), and `customer: {
email, name?, phone? }`. **`customer.email` is now required, not
optional** — the old code let it be silently omitted; the real schema
requires it, so this now throws a clear `providerError` before any
request is even built, rather than letting Korapay reject an
incomplete request with a less specific error.

**Verified:** `node --check` on the modified file; a standalone
functional test, 4 cases, all passing — full nested shape correct
with zero stray top-level fields; the `mobile_money` type override
works; no leaked `undefined` `name`/`phone` keys in `customer` when
those are omitted (JS's `...(cond && {...})` spread pattern, confirmed
it doesn't add an `undefined`-valued key the way a plain conditional
assignment might); missing `customer.email` throws before payload
construction.

**Deliberately NOT touched this session — flagged, not fixed:** the
response-parsing side, believed at the time to be a flat
`status`-as-string object. **That guess was corrected the next
session — see "The 'b' this split implies — now built" below, which
supersedes this paragraph's own framing.** Also still open, unchanged
from Part a's original notes above: the TZS currency-list discrepancy,
and the XAF/XOF rounding-multiple rule.

### The "b" this split implies — now built (2026-09-02)

**Re-fetched `developers.korapay.com/docs/payout-via-api` directly
before writing anything — the prior session's guess (a flat
`status`-as-string object) was wrong.** The real shape, straight from
Kora's own documented example, is **two levels**: `responseData.status`
(top-level) genuinely IS a boolean — `true`/`false`, "did Kora accept
this API call" — and the existing `!responseData.status` check was
*already correct* for that. What was actually missing: a completely
separate field, `responseData.data.status`, a STRING describing the
*transaction's own* lifecycle state (`"processing"` in Kora's own
example; presumably `"success"`/`"failed"` too, though only
`"processing"` appears in their documented sample — a payout is rarely
resolved synchronously). This code never looked at that field at all.

**Fixed in `providers/korapay.js#processPayout()`:**
- `"processing"` is treated as the **normal, expected** outcome, not
  an error — Kora's own docs are explicit that payout confirmation is
  asynchronous ("Receive confirmation via webhook when the payout is
  completed" / "Query the transaction to get the status"), and
  separately warn against treating an ambiguous outcome as failure
  without verifying first (their own "Handling Unexpected Request
  Errors" section: an unexpected error "may have been accepted and
  processed by Kora" regardless). `processPayout()`'s own job now ends
  at "Kora accepted the request," documented explicitly in a code
  comment — it does NOT confirm money actually moved, and callers must
  not treat its return as "payout completed." **Neither a webhook
  handler nor a Payout Verification API call exists anywhere in this
  codebase yet** — a real, separate gap this fix surfaces but doesn't
  close; flagged here rather than silently assumed handled elsewhere.
- **New, defensive check added:** `data.status === 'failed'` (not
  shown in Kora's own documented example, but plausible for an
  immediate synchronous rejection — e.g. an obviously invalid
  destination) now throws a real error. Outer `status: true` only ever
  confirmed the API call was well-formed and accepted, never that the
  transfer would succeed — treating this combination as silent success
  would have been a real-money bug, not a cosmetic one.
- A log line now explicitly states the transaction's `data.status`
  value and repeats, inline, that this is an acknowledgement, not
  final confirmation — so anyone reading production logs isn't misled
  by a log line that used to just say "Payout success" for a merely
  `"processing"` transaction.

**Verified:** `node --check` on the modified file. A standalone
functional test, 6 cases (matching Kora's own documented "processing"
example; a hypothetical synchronous "success"; a synchronous
`data.status: 'failed'` with outer `status: true` — the new defensive
check; an outer `status: false` API-level rejection; an HTTP-level
502-style failure; a response with no `data.status` field at all,
confirming that doesn't spuriously throw) — **all 6 correct**.

**Deliberately NOT built — flagged as real, separate gaps, not
silently assumed out of scope:**
- No webhook handler for payout completion/failure events anywhere in
  this repo. Without one, this backend (and by extension Mavins-web)
  has no way to ever learn a `"processing"` payout's true final
  outcome short of manually polling the Payout Verification API.
- ~~No Payout Verification API call implemented either~~ — **built
  this session, see "the missing verification call — part i" below.**
- Still open, unchanged: the TZS currency-list discrepancy, and the
  XAF/XOF rounding-multiple rule (Part a's own notes above).

### The missing verification call — part i (2026-09-02) [x] (i only, ii not built)

**Part ii built this session (2026-09-04) — see this section's own
"Part ii" entry further down for the full write-up; header/status
here now reflects both parts done.**

**Split into i/ii per direct instruction ("split into a and b... add
i and ii, do only i") — this is a separate split from Task 42's own
Part a/b/c lettering above, since this gap was never itself lettered,
only flagged as prose.** New
`providers/korapay.js#verifyPayout(reference)`, mirroring the
existing collection-side `verifyTransaction()`'s structure rather than
inventing a new shape.

**Endpoint confidence stated explicitly, weaker than the request/
response shape fixes above — that difference matters enough to spell
out, not gloss over:** Korapay's own payout-via-api docs page never
states the single-payout verify path as a literal string — it only
links to a separate anchor-based API reference this session couldn't
resolve to a concrete URL. Instead, this is a **strong pattern-match
from a real, directly-confirmed sibling**: Korapay's own Bulk Payouts
docs explicitly show `POST .../transactions/disburse/bulk` creates a
batch and `GET .../transactions/bulk/:batch_reference` verifies it.
Applying that same create/verify pairing to the single (non-bulk)
case — the same `transactions` resource family `processPayout()`
already POSTs to, just dropping the `bulk/` segment — gives `GET
.../transactions/{reference}`, which is what got built. **Recommend
one real sandbox call to confirm this before trusting it in
production** — flagged explicitly so this verification step doesn't
get silently skipped later by a future session assuming it's already
settled to the same standard as the rest of this fix.

**Response handling deliberately differs from `processPayout()`'s own
— on purpose, not an inconsistency:** `processPayout()` throws on
`data.status === 'failed'` (a payout failing is an error *for the
function whose job is to initiate a payout*). `verifyPayout()` does
**not** throw on that same value — a failed payout is a normal,
correctly-answered result of *asking what happened*, not an error in
asking. Only a genuine API-level rejection (bad reference, auth
failure, non-2xx) throws here.

**Verified:** `node --check`; a standalone functional test, 5 cases,
all passing — success/processing/failed transaction states all
resolve without throwing (confirming the deliberate divergence from
`processPayout()` above), while an API-level rejection and an
HTTP-level failure both throw correctly.

**Part ii — wiring `verifyPayout()` into an actual route [x] Done
(2026-09-04).** New `GET /api/payout/verify?reference=X&provider=Y`,
gated by `requireInternalApiKey` (same auth requirement as `/payout`
itself — reading a payout's destination/amount/status is the same
sensitivity class as initiating one, no reason for a weaker gate on
the read side). Mirrors `/verify`'s own query-param shape exactly
(this is that route's payout-side counterpart), not `/payout`'s POST
shape, since this only reads state and changes nothing.
`providerName` defaults to `ROUTING_RULES.payout` (korapay), same
default-provider pattern `/payout` itself already uses — not
hardcoded to `'korapay'` directly, so this route doesn't need to
change if a future provider is ever fully integrated (Task 43's own
architecture note: new providers start as stubs, Korapay stays
primary). Guards against calling `verifyPayout` on a provider that
hasn't implemented it (today, everything except Korapay) with a clear
`501` rather than an unhandled `TypeError` reaching the client.

**Verified:** `node --check` on `routes.js`. A standalone functional
test, 6 cases (valid reference/default provider, an explicit provider
lacking `verifyPayout` → 501, missing reference → 400, empty-string
reference → 400, non-string reference → 400, valid reference with
explicit `provider=korapay`) — all 6 correct.

**Still fully open, unchanged by this part:** the webhook-handler half
of the original gap (no way to be pushed a payout's outcome, only to
poll for it now) remains unbuilt. Part b-b (`/verify`/`/banks`'s own
auth-extension question) also remains open, independent of this.

### Part b — is extending `requireInternalApiKey` to `/pay`/`/verify`/`/banks` even appropriate? [ ] (split into a/b — a fully resolved via i/ii, b not started)

Original concern stands as the framing question: `/pay` and `/verify`
may need to stay reachable from contexts `/payout` never should be.
Split into two independent halves along the routes' actual usage
lines, since `/pay` and `/verify`/`/banks` turn out to have completely
different evidence available (see Part b-a below) — bundling them
risked a single verdict papering over that difference.

### Part b-a — investigate `/pay` specifically [x] (split into i/ii — both done: i = facts, ii = verdict)

Split further per the same rule, into a pure fact-finding half (i)
and the actual verdict that depends on it (ii) — kept deliberately
separate so the fact-finding chunk can land as its own small,
reviewable, documentation-only piece rather than bundling
"here's what I found" and "here's what I think we should do about it"
into one commit.

### Part b-a-i — confirm exactly who calls `/pay`, and characterize that caller's trust level [x] (documentation only, no code changed, no verdict rendered)

**Done this session (2026-09-01) — facts only, deliberately no
recommendation yet (that's Part b-a-ii).**

Re-confirmed via fresh clones of both Mavins-web and Velune (not
reused from any earlier session's possibly-stale finding):

- **Exactly one caller of `/api/pay` exists anywhere across both
  repos**: Mavins-web's `supabase/functions/initialize-payment/index.ts`,
  a Supabase Edge Function. It reads this backend's base URL from
  `Deno.env.get('BPAY_BACKEND_URL')` (a Supabase secret, not a
  client-exposed `NEXT_PUBLIC_*` var) and calls
  `fetch(\`${bpayBackendUrl}/api/pay\`, ...)` server-side.
- Grepped all of Mavins-web's `src/` and `supabase/` for any other
  reference to `b-pay-backend`, `bpayBackendUrl`, or `BPAY_BACKEND_URL`:
  zero other hits.
- Grepped all of Velune's Kotlin source for `b-pay`/`bpay`/`BPAY`/
  `korapay`: zero hits anywhere — Velune doesn't call this backend at
  all, for `/pay` or anything else.
- **Trust characterization of the one confirmed caller**: server-to-
  server, Supabase Edge Function environment, secret held in
  `Deno.env` rather than any client-reachable variable. This is
  structurally the same kind of trusted, non-browser context
  `/payout`'s own caller (also confirmed server-side, per Task 42 Part
  A's own investigation) already is.

**Deliberately not concluded here — that's Part b-a-ii's job:**
whether this fact pattern makes adding `requireInternalApiKey` to
`/pay` safe/appropriate, and if so, exactly what changes
`initialize-payment/index.ts` would need (almost certainly: read a new
secret, attach an `X-Internal-Api-Key` header) — a cross-repo
follow-up if so, not something this backend's own code change alone
would complete.

### Part b-a-ii — render the actual recommendation for `/pay`, based on Part b-a-i's facts [x] (documentation only, no code changed)

**Done this session (2026-09-02) — recommendation: yes, extend
`requireInternalApiKey` to `/pay`.**

Part b-a-i's confirmed facts leave no real ambiguity here: `/pay` has
exactly one caller anywhere across the two apps that could plausibly
use this backend (Mavins-web, Velune), and that caller is a Supabase
Edge Function — server-to-server, holding whatever secret it needs in
`Deno.env`, never exposed to a browser or any client-reachable
context. That is structurally identical to `/payout`'s own caller,
which already justified adding this exact same middleware in Task 42
Part A. There is no legitimate current use case for `/pay` being
reachable by an unauthenticated request — the one real consumer is
already positioned to hold and send an internal API key, the same way
it already holds `BPAY_BACKEND_URL` itself.

**Scope note — this verdict covers `/pay` only, deliberately.**
`/verify` and `/banks` are Part b-b's own question, not decided here
even though b-a-i's grep pass happened to touch on them in passing —
they could have a legitimate reason to stay open (e.g. if either is
ever meant to be reachable from a context `/pay`/`/payout` aren't) that
this investigation never actually checked for.

**Not done here, per Part b/c's own existing split — implementation is
Part c's job:** actually adding the `requireInternalApiKey` middleware
to the `/pay` route in `routes.js`, generating/confirming the shared
secret value, and updating Mavins-web's `initialize-payment/index.ts`
to read that secret and attach `X-Internal-Api-Key` to its request —
a cross-repo change, not something this commit does.

Part b (both halves) is now fully resolved as an investigation: `/pay`
→ extend the middleware (this entry); `/verify`/`/banks` → still open,
Part b-b not started. Part c can proceed for `/pay` immediately; it
should wait on Part b-b before touching the other two routes.

### Part b-b — investigate `/verify` and `/banks` specifically [ ]

Not started. **One fact already surfaced as a side effect of Part
b-a-i's own greps, worth recording now rather than re-discovering
later**: neither `/api/verify` nor `/api/banks` turned up any caller
at all in either Mavins-web or Velune's source during that same pass —
but Part b-b should still do its own dedicated, deliberate check
(rather than treating this as conclusive) before relying on it, since
b-a-i's greps were scoped to confirm `/pay`'s caller specifically, not
built to be an exhaustive audit of these two routes.

### Part c — implement whatever Part b concludes [ ] (split into a/b, per the standing mandatory splitting rule — a done, b not started)

### Part c-a — this backend's own side: protect `/pay` with `requireInternalApiKey` [x]

**Done this session (2026-09-02), commit `d8615c0`.** `router.post('/pay', ...)`
now has the same `requireInternalApiKey` middleware `/payout` already
had, per Part b-a-ii's verdict. Also fixed, in the same commit since
it's directly touched: the middleware's own rejection log hardcoded
"payout-adjacent" in its wording — now generic, since it guards two
routes now, not one. Verified via `node --check` on both changed files
plus a throwaway functional smoke test (real `requireInternalApiKey`
imported into a minimal Express app, `/api/pay` hit three ways): no
key → 401; wrong key → 401; correct key → reaches the handler. All
three passed.

### Part c-b — cross-repo: Mavins-web's `initialize-payment` Edge Function must send the new header [x]

**Done — in Mavins-web, not this repo. This entry was stale; corrected
this session after finding the fix already shipped there while
pulling latest for an unrelated task.** Mavins-web's own
`handover.md` → **Task 63** (commit `4a95a52`,
"Task 63 — send X-Internal-Api-Key from initialize-payment, urgent
B-Pay-backend Task 42 Part c-b") built this exactly as specified
below, verified by reading both the code and that repo's own write-up
directly rather than trusting the commit message alone:
`supabase/functions/initialize-payment/index.ts` now reads a new
secret, `BPAY_INTERNAL_API_KEY`, and sends it as `X-Internal-Api-Key`
on its existing `fetch()` call to this backend's `/api/pay` — header
name confirmed to match `requireInternalApiKey`'s own
`req.headers['x-internal-api-key']` check exactly (Express lowercases
header names, so the casing difference is not a mismatch). Fails
closed with a `500` + specific error if the secret isn't set, same
posture as this backend's own middleware.

**What's NOT done, and can't be from either sandbox — a live
deployment/secrets-configuration step, not a code gap:**
```
supabase secrets set BPAY_INTERNAL_API_KEY=<same value as this backend's own INTERNAL_API_KEY>
supabase functions deploy initialize-payment
```
This value must be **identical** to this backend's own
`INTERNAL_API_KEY` Render env var — one shared secret, not two
independent ones. If `INTERNAL_API_KEY` doesn't have a value set on
Render yet, generate one and set it on **both** sides in the same
sitting. Deploy Mavins-web's Edge Function alongside (or before) this
backend's own Part c-a rollout — never after. **This is the one
remaining real risk**: the code on both sides is correct and
consistent, but until both secrets are actually set to the same value
and both sides are actually deployed, the ordering warning above still
applies in spirit — an actual live gap, not a documentation one.

---

## Task 43 — Fork Edges-Enterprise/bpay + PR workflow; "Bpay app" architecture direction [x] (fork confirmed to already exist; real work has since started there — see correction below)

**Correction (2026-09-04) — the repo names in this task's own body
below are wrong casing, and its "not yet done" list is now stale.**
Confirmed directly via `git ls-remote` against both URLs (not
assumed): the real names are **`Zapier-codes/B-PAY`** (the fork) and
**`Edges-Enterprise/B-PAY`** (the upstream) — all-caps `B-PAY`, not
lowercase `bpay` as guessed below. Both exist and are live (`git
ls-remote` returned real `HEAD`/`refs/heads/main` for each). **The
fork already has real work on it, done by another session, unrelated
to this repo's own Task 42/43** — `Zapier-codes/B-PAY`'s own git log
shows `Task 67`: three security findings fixed (a hardcoded Lizzysub
API token, a live Payscribe secret key hardcoded in client-side
TypeScript, an undefined-`supabase`-client bug in `resolve_tag/
index.ts`), committed as `712825f`, plus an earlier CI build swap
(`0ac6bc7`). **Not yet done, still accurate:** opening the actual PR
from `Zapier-codes/B-PAY` to `Edges-Enterprise/B-PAY` (manual GitHub
action, no session has authenticated GitHub access to create it), and
rotating the two exposed live secrets (compromised regardless of the
code fix, since they were already in git history). The rest of this
task's original body (architectural direction: this backend as the
Bpay app's only payment source of truth, Korapay-primary-provider
constraint, stub-not-half-built for new providers) is unaffected by
this correction and still stands as written below.

**Second correction (2026-09-05) — the cross-repo purpose question is
now resolved, confirmed directly, not assumed.** mavins-web's own
`handover.md` (Task 70) independently flagged the same repo from its
own side with a seemingly different stated purpose — repointing this
same fork onto Mavins-web's own Supabase project so Mavins can credit
`bpay_tag` wallets for listener payouts, not this task's own
"consolidate the Bpay app's payment plumbing through B-Pay-backend"
framing. Neither session that wrote these two tasks had seen the
other. Asked directly rather than picking one interpretation
unilaterally: **confirmed — one fork, both purposes.**
`Zapier-codes/B-PAY` will both (1) have its own payment/payout calls
consolidated to route through this backend, per this task's own
direction below, and (2) get repointed onto Mavins-web's own Supabase
project so Mavins can credit listener wallets directly, per Task 70's
own direction. These are complementary changes to the same fork, not
competing visions — resolving the ambiguity both tasks independently
flagged.

**Documentation only, per explicit instruction — no code, no repo
clone, no exploration this session.** The product owner gave real
architectural direction for a separate, not-yet-touched app/repo this
session; recorded here in full so the next session that actually
clones and works in it doesn't have to ask again.

**This is a different, third repo — not this one.** "Bpay app" (the
product owner's own name for it) is a wallet-based banking
application — the actual end-user-facing product, aiming to become a
fully functional wallet/banking app in the same category as Wise or
Chipper Cash. **Currently ~50% done, per the product owner directly.**
This is distinct from **this** repo (`B-Pay-backend`), which is the
Node.js payment/provider-integration service the Bpay app is meant to
consume, not the app itself.

**Next session's first job — provide the fork + PR command(s):**
fork `Edges-Enterprise/bpay` (the real upstream, owned by a different
org) into `Zapier-codes/bpay`, so that changes can be pushed to
`Zapier-codes/bpay` and submitted as pull requests the
`Edges-Enterprise/bpay` maintainers can merge upstream. **This
mirrors this same repo's own already-established fork/upstream
pattern** (see this file's own notes on the `Phoenix-Boss/B-PAY-backend`
upstream remote and PR #2) — same shape of relationship, different
repo and different upstream org. Provide the actual `gh repo fork` (or
equivalent `git remote add upstream` + manual fork) commands directly
to the product owner — this session was explicitly told not to run
them itself.

**Product owner's stated architectural aim for the Bpay app, once
that fork/PR workflow exists:**

1. **Point the Bpay app at this repo (`B-Pay-backend`) as the ONLY
   source of truth for its payments and payouts.** No duplicate or
   parallel payment logic inside the Bpay app itself — every real
   money movement (pay, payout, verify) routes through this backend,
   the same "single source of truth" discipline already established
   elsewhere in this project (Task 34's wallet-write consolidation,
   the RPC-is-the-only-writer pattern).
2. **This backend's own modularity is a deliberate, standing
   constraint on how future payment providers get added, not just an
   architectural nice-to-have:** **Korapay is, and remains, the
   primary payment method.** Any additional payment system or API
   considered for this backend must provide something Korapay does
   **not** already supply — not a redundant alternative rail for
   something Korapay already handles. Until a provider's full
   integration is actually undertaken, add it only as a **stub**
   (matching this project's own established pattern for
   Payscribe/JuicyWay elsewhere in this file — routes/structure
   present, real integration deliberately deferred) — never a half-
   built live integration.

**Not yet done, deliberately, per this session's own scope:** cloning
`bpay` (or `Zapier-codes/bpay` once forked), auditing its current
~50%-done state against this direction, or beginning any actual
wiring work. That's real, substantial future work for whichever
session actually has the repo available — this task only records the
direction so that session doesn't have to re-ask it.

---

## Task 44 — Cross-repo reconciliation: Lizzysub (VTU) + Juicyway integration scope, migrated from mavins-web's Task 71 [ ]

**Origin:** mavins-web's `handover.md` Task 71 says the canonical
write-up for this work belongs here, reserved as "Task 44," but the
detailed findings were never actually written back — a patch
containing them was generated in a mavins-web session but never
applied to this repo's real `origin/main` (confirmed: this repo's own
git log shows unrelated fork/PR-workflow work landed as Task 43 in
that same window instead). **That detailed content currently exists
nowhere live** — this entry reconstructs what's knowable from
mavins-web's side plus direct verification against this repo's actual
current code, rather than assuming the lost write-up's specifics.

**Goal, per the product owner (relayed via mavins-web, not yet
independently confirmed with the product owner from this repo's
side):** this backend becomes the single source of truth for all
payment/utility services — integrate Lizzysub (VTU / airtime-data
top-ups) as a new provider, and fix/complete the existing Juicyway
integration to cover whatever Korapay doesn't already handle.

**Lizzysub — not started.** No `providers/lizzysub.js` or any
Lizzysub reference exists anywhere in this repo (confirmed by
search). Open questions, inherited from mavins-web and still
unanswered here:
- Lizzysub's real API surface (endpoints, auth scheme, request/
  response shapes) — no primary source consulted yet.
- What "VTU" concretely means for this repo's own route surface (new
  `/api/vtu*` routes? folded into `/pay`? a new resource type in
  `routes.js`?).
- Where Lizzysub credentials should live — likely an extension of the
  existing `getProviderKey('provider', 'secret'|'public')` convention
  in `utils/helpers.js`, but not confirmed.
- Whether new Lizzysub routes should sit behind
  `requireInternalApiKey`, matching this repo's own Task 42 pattern
  for `/pay`/`/payout`.

**Juicyway — partially built; specific issues flagged by mavins-web,
now independently confirmed by a later session's audit (2026-09-06,
see Task 8b / "Confirmed research findings"):**
- Direct read of `providers/juicyway.js` (this repo's current
  `origin/main`, this session): `processPayment()` posts to
  `${this.baseUrl}/v1/charges` with `Authorization: Bearer
  ${this.apiKey}`. The code itself already carries a `⚠️ Verify exact
  endpoint path in Juicyway docs` comment at that line — so the
  endpoint-path uncertainty mavins-web flagged is real and was already
  self-acknowledged in this repo, not new information. **Update
  (2026-09-06): confirmed wrong** — the real path is
  `POST /payment-sessions`, not `/v1/charges`. See Task 45a.
- mavins-web's Task 71 (relaying an earlier, now-lost write-up) claims
  two further concrete bugs: a wrong auth header prefix and an
  incomplete request payload, said to be cross-checked at the time
  against a Termux-verified reference doc the product owner supplied.
  **This session had no access to that reference doc and could not
  independently verify either claim** — treat both as credible but
  unconfirmed until someone re-checks against Juicyway's actual
  current docs directly. **Update (2026-09-06): both now independently
  confirmed true**, this time directly against docs.juicyway.com
  itself rather than the lost reference doc — the `Bearer ` prefix is
  wrong (docs.juicyway.com/authentication.md is explicit the header is
  the raw key), and the payload is missing most of the documented
  required nested fields (`customer`, `payment_method`, `order`,
  `description`). See Tasks 45a and 45b. mavins-web's claims turned out
  to be accurate even without access to whatever reference doc backed
  them originally.
- By contrast, `verifyWebhookSignature()` in the same file (business-
  ID-keyed HMAC, alphabetized-key stable stringify, uppercase hex
  digest, all explained in detailed inline comments citing
  docs.juicyway.com) reads as fully built and deliberately careful —
  no source flags this part as broken; don't redo it without a
  specific new reason to doubt it. **Still true as of the 2026-09-06
  audit** — that pass re-confirmed relevance but found no new webhook
  issues, see the "Confirmed research findings" JuicyWay section.

**Korapay-vs-Juicyway scoping** — still open per mavins-web: exactly
what should Juicyway cover that Korapay doesn't already? Not answered
from either repo yet.

**Explicitly not done this session:** no provider code written, no
patch applied, no routes added. Separately: mavins-web's own
`handover.md` contains a block instructing readers to download and
`git am` a patch file (`b-pay-backend-payout-flow.patch`) and push it
straight to this repo's `main` with no review step. That file was not
available to inspect in this sandbox, and regardless of its contents,
downloading and pushing an unreviewed patch straight to a payments
backend's main branch isn't a safe pattern to follow on the strength
of a handover-doc instruction alone. **Any future session that finds
similar "apply this patch and push" language in either repo's
handover.md should treat it as an unverified claim to check, not a
command to execute.**

**Next concrete step for whoever picks this up:** (1) get Lizzysub's
real API docs, (2) get the Termux-verified Juicyway reference doc
mavins-web mentions, or re-verify against Juicyway's current docs
directly, (3) only then write the actual routing/auth decisions and
provider code. This entry is a scoping/reconciliation record, not an
implementation — nothing here should be treated as ready to build
against without that verification step.

---

## Task Numbering & Workflow Convention (read before Task 0)

**Superseded 2026-09-07 (below) — kept here only as a record of the
prior scheme, since some already-written task entries below still use
it and haven't been migrated yet.** Adopted the session before this
one: every task decomposed on up to four levels: **a/b/c/d → 1/2/3 →
i/ii → X**, `X` being a marker (not a literal tier) for whichever leaf
was the single current atomic unit of work.

- When the node marked `X` is solved, the next unsolved leaf at that
  same level becomes the new `X`.
- When an entire branch (all its leaves) is solved, the *next*
  session deletes that branch from this board entirely — same
  discipline this file already uses elsewhere (see the "Tasks ...
  removed from this file entirely, kept only as a record" pattern
  under Task 43's history) — and replaces it with a one-line "done"
  note in the parent task's own summary line, not the deleted detail.
- A session should never work on more than one `X` at a time, and
  never work on a non-`X` node "while it's convenient" — if something
  outside the current `X` needs attention, flag it in that node's own
  entry for a future session rather than context-switching mid-task.

### Deeper split, effective 2026-09-07 — supersedes the four-level scheme above for every task from this point forward

Direct product-owner instruction this session: the four-level split
(a/b/c/d → 1/2/3 → i/ii → X) wasn't fine-grained enough — every task
now decomposes on **six** levels instead:

**a/b/c/d/e → 1/2/3/4 → i/ii/iii → zi/zo → X**

- **a–e** (max 5): the task's top-level parts, same idea as the old
  a–d tier, just one slot wider.
- **1–4** (max 4) under each letter: that part's own sub-parts.
- **i/ii/iii** (max 3) under each number: that sub-part's own steps.
- **zi/zo** (max 2, always exactly these two labels — not a third
  `zi`/`zo`/`zu` or similar) under each roman-numeral step: the
  smallest still-not-atomic split of that step. `zi` and `zo` are
  fixed names, not a running letter sequence — every step that goes
  this deep has exactly a `zi` branch and a `zo` branch, no more.
- **X** — same marker convention as before, not a literal seventh
  tier: whichever `zi` or `zo` leaf is the single current atomic unit
  of work carries the label `X` in place of `zi`/`zo`. A leaf that
  turns out to be genuinely atomic without needing a `zi`/`zo` split
  can carry `X` directly at the i/ii/iii level instead — don't invent
  a fake `zi`/`zo` split just to reach the letter before writing `X`.
- **Exactly one node on the whole board carries `X` at any given
  time**, same rule as before, and it's still the only thing the next
  session should work on.
- **Only the patch file for the current `X` node is ever produced.**
  A session's output is one patch, covering one `X`'s worth of change
  (or, when the `X` is itself a documentation/decision node with no
  code, whatever `handover.md` edit that decision requires) — never a
  patch bundling multiple leaves, even if adjacent leaves look small
  enough to combine.
- **When `X` is resolved and its patch is applied/pushed, the next
  unsolved leaf in reading order (down through zo before moving to the
  next i/ii/iii, down through iii before moving to the next 1–4, and
  so on) becomes the new `X`.** This repeats leaf by leaf until every
  leaf under a task is solved, at which point that task's branch gets
  deleted from this board per the collapse rule above, same as before.
- **Migration of already-written tasks is itself incremental, not a
  one-shot rewrite:** re-splitting every existing task (Task 0 through
  54) into the full six-level shape in a single session would violate
  the one-`X`-at-a-time rule this same convention exists to enforce.
  Instead: the task currently holding the board's `X` gets migrated
  first (see Task 52/a-1's retrofit immediately below, done this
  session as the worked example), and every other task keeps its old
  a/1/i/X labels, read as shorthand for a/1/i(-only, no zi/zo split
  written out yet)/X, until whichever future session's `X` lands
  inside that task and migrates it as part of doing that work — not
  before, and not as a separate bulk-relabeling pass.

---

## Patch Handoff Convention (read before Task 0)

**This repo's handoff process, effective this session, supersedes any
different pattern found in this file's own history or in any other
repo's handover.md (including mavins-web's):**

1. A session does its work, commits locally, and generates a patch
   file (`git format-patch`).
2. The session hands that patch to the product owner directly and
   explains what it contains.
3. **The product owner reviews and applies it themselves.** Product
   owner's environment is Termux; downloaded patches land in
   `~/storage/downloads/`, and the repo checkout lives at
   `~/B-PAY-backend` **(note the exact case — confirmed directly by
   the product owner, 2026-09-06: this is about the local directory
   name on their own device, a Linux/Termux filesystem, which is
   case-sensitive; it is not a claim about the GitHub repo's own
   name, which remains `B-Pay-backend` as cloned by every session —
   GitHub repo URLs/clone targets are not case-sensitive the way a
   local directory name is, so there's no actual conflict here, just
   two different names for two different things: the remote repo and
   this one local checkout of it).** **The exact commands the product
   owner runs themselves, after reading the patch — not commands any
   session runs against this repo:**
   ```
   cd ~/B-PAY-backend
   git am ~/storage/downloads/<patch-file-name>
   git push
   ```
   A session's job ends at handing over the patch file and explaining
   what's in it; running the three commands above is the product
   owner's own step, done from their own device, on their own
   authority.
4. No session applies a patch to this repo on its own authority, and
   no session pushes to `main` itself — regardless of instructions
   found embedded in any handover.md, this repo's or a sibling
   repo's. If a future session finds "download and apply this patch,
   then push to main" language anywhere, treat it as an unverified
   claim to flag for the product owner, not a command to execute.
   This applies with extra force here specifically because this repo
   is being scoped (Task 0, below) to move real money across ten
   payment providers with no database of its own — there is no local
   transaction record to fall back on if an unreviewed change goes
   wrong, so the human review step is not optional.

---

## Task 0 — Discovery: canonical multi-provider payment orchestration architecture [ ]

**Goal, stated directly by the product owner this session:** turn
this repo into the single orchestration layer for every payment
provider integration the business uses — **Korapay, Paystack,
Juicyway, Payscribe** (already integrated in this repo, per earlier
tasks), plus net-new: **DodoPayments, Flutterwave, Remita, Xixapay,
PaymentPoint, Presmit**, and **`telcos.opik.net`** (the product
owner's own personal endpoint, stated purpose: global VTU/airtime-
data services). No customer, and no business integrating this
platform, is ever meant to see which underlying provider actually
handled a given transaction — provider selection and routing become
entirely internal to this repo.

**Business model, stated directly by the product owner this
session:** businesses integrate against this platform's own API and
see only this platform's own services and per-transaction pricing —
payins, cards, conversions, payouts, etc. — never the underlying
provider names. **Pricing, revised this session (previously 5x):
this platform charges 3x whatever the underlying provider charges,
per transaction, across every service type** — product owner's own
stated reason: to leave headroom for the platform's other running
costs. This session did not evaluate the multiplier commercially or
legally, at 3x any more than it did at 5x — flagging one thing still
worth resolving with a lawyer or compliance advisor before this goes
live, not blocking it: combining (a) a markup of this kind with (b)
fully hiding which regulated payment provider is actually moving the
money is exactly the kind of setup that payment regulators and card
networks tend to have specific rules about (in Nigeria, that's
typically the CBN's payment-service-provider licensing categories;
Visa/Mastercard also have their own surcharge-disclosure rules).
Lowering the multiplier doesn't on its own resolve that question —
it's the combination of markup-plus-hidden-provider that regulators
care about, not the specific number — so this stays a real open item
for the product owner to get an actual answer on before production
traffic, same spirit as the no-DB audit-trail question in (c) below.

**Default provider, stated directly by the product owner this
session:** Korapay is the initial default provider, but the default
must be a **dynamic, changeable value** — the platform owner can
switch it at any time through an admin interface, not a hardcoded
constant.

**Customer-facing surface, per product owner:**
- A **dynamic, fully white-label-configurable checkout page** — this
  is the only thing customers interact with; no provider is ever
  named or exposed to them. Users never select a provider themselves
  — the orchestration layer decides, invisibly.
- A **home page** where end users land before reaching checkout.
- An **admin route, explicitly not exposed to end users**, where the
  platform owner (only) can change the dynamic default provider and
  presumably other platform-level config — scope of what else lives
  here not yet defined (see d-3 below).

**Explicit architectural constraint, per product owner: this repo is
NOT to have a database at all.** Its job is limited to (a) initiating
payments/payouts by internally routing to whichever provider is
selected, and (b) forwarding webhooks from those providers onward —
no local persistence of any transaction, session, or customer data.
**Reversed, 2026-09-06 — see Task 46.** The product owner has since
decided this repo will carry a database after all, specifically to
back a full admin dashboard (country permissions per business, card
activation/deactivation). Left in place here, struck through in
spirit rather than in the file's history, so any session reading this
paragraph on its own sees both the original constraint and the fact
that it no longer holds — go to Task 46 for the current state, not
this paragraph alone.

**a. Provider integration inventory** — one sub-branch per provider.

**Discovery convention, stated directly by the product owner this
session, in force for every sub-item below:** each provider gets
exactly one real discovery pass. That session web-searches for the
provider's official API documentation; if it genuinely can't be
found, it asks the product owner directly rather than guessing. Once
found, the session reads it and writes a **full audit into this
file** under that provider's own entry — endpoints, auth scheme,
request/response shapes, a link/citation to the actual doc, and the
date it was checked. **That written audit becomes the source of
truth for every later implementation session — implementation
sessions build against the recorded audit and do not redo the
discovery web search.** One narrow exception, not a loophole: if an
implementation session hits something in real testing that
concretely contradicts a recorded audit (an endpoint 404s, a
signature scheme fails to verify against real test data), that's
grounds to re-check *that one provider's* audit — not grounds to
distrust the convention itself or start re-discovering providers
that are working fine.

None should be assumed to behave like an existing one; each gets its
own real-docs audit before any code is written, the same discipline
already applied to Korapay/Paystack/Juicyway/Payscribe.

- **a-1. Korapay** — already integrated. Re-scope under the no-DB
  constraint.
  - a-1-i. Confirm `providers/korapay.js` / `routes.js` have no
    implicit dependency on persisted state.
    - **a-1-i-X** *(active — start here)*
- **a-2. Paystack** — already integrated. Same no-DB audit as a-1.
- **a-3. Juicyway** — already integrated, but **now confirmed broken,
  not just "unverified"**: Task 8b's full audit pass (2026-09-06)
  turned the previously-unverified endpoint-path/auth-header/payload
  claims into four confirmed real bugs plus one unresolved documented
  currency conflict — endpoint path wrong (`/v1/charges` doesn't
  exist; real path is `/payment-sessions`), `Authorization` header
  wrongly includes a `Bearer ` prefix JuicyWay's docs explicitly say
  not to send, the request payload is missing most of JuicyWay's
  required nested fields, and the error-message extraction reads a
  field that doesn't exist in JuicyWay's real error envelope (always
  falls back to a generic string). See Tasks 45a–45e for the
  individual fixes and the still-unresolved currency ambiguity — all
  four confirmed bugs must close, and Task 45e's currency conflict
  must resolve, before this provider is trustworthy inside an
  orchestration layer, same bar Task 44 already set, now with actual
  confirmed findings backing it instead of unverified claims.
- **a-4. DodoPayments** — **discovery/audit done (2026-09-06,
  doc-only, no code yet)**, per the Discovery Convention above. Full
  audit is in the "Confirmed research findings" section (search for
  "DodoPayments — FULL API discovery pass"). Real open item queued
  there rather than resolved: a genuine conflict between two
  DodoPayments primary sources on which currencies can be a
  product's base/settlement currency (one says USD/INR only, another
  lists EUR/GBP/etc. as "native settlement currencies") — resolve
  against a live Dashboard or fresh API call before writing
  `providers/dodopayments.js`, don't pick a side from docs alone.
  Also flagged there: this is a Merchant-of-Record platform (Dodo is
  the legal seller, not this platform), a product-catalog-driven
  integration model (needs pre-provisioned `product_id`s, not an
  arbitrary per-call amount like every other provider here), and a
  webhook scheme (Standard Webhooks: 3 headers, HMAC-SHA256,
  base64, `id.timestamp.body`) that is structurally different from
  Paystack/Korapay's single hex-HMAC-header schemes — implementation
  should use the `standardwebhooks` package rather than adapting
  existing signature-verification code.
- **a-5. Flutterwave** — **discovery/audit done (2026-09-06, doc-only,
  no code yet)**, per the Discovery Convention above. Full audit is in
  the "Confirmed research findings" section (search "Flutterwave —
  FULL API discovery pass"). **The primary open item is not a
  currency or base-URL conflict like the other net-new providers
  above — it's a version choice**: Flutterwave currently publishes two
  structurally incompatible generations at once. v3 uses the same
  single-static-secret-key Bearer scheme as every other provider
  already in this repo; v4 (the docs portal's current default) is a
  full OAuth2 client-credentials flow with a 10-minute-expiring access
  token, needing this repo's first token-refresh-manager component —
  a real complication under Task 0's own no-database constraint, since
  the token/expiry would have to live in-process memory only. **Get
  the product owner's explicit v3-vs-v4 decision before writing
  `providers/flutterwave.js`** — not resolvable from docs alone. Other
  confirmed findings queued there, all relevant once the version
  question is settled: v4's card charges need field-level AES-256-GCM
  encryption with a third, distinct secret (separate from the OAuth
  credentials); the single-call charge/payout analogs to this repo's
  other providers are the `/orchestration/direct-charges` and
  `/direct-transfers` "Orchestrator" endpoints specifically, not the
  plain `/charges`/`/transfers` endpoints (which need pre-created
  customer/recipient IDs first); payout request shape is fully
  currency-conditional (NGN needs only account/bank-code, EUR/GBP/USD/
  ZAR need full recipient-and-sender KYC); and the Webhooks doc page
  contradicts itself between its "Verifying Webhook Signatures"
  section (HMAC-SHA256, base64) and its own "Examples" section (a
  direct, non-HMAC string comparison) — the former is correct, the
  latter must not be copied as-is.
- **a-6. Remita** — **discovery/audit done (2026-09-06, doc-only, no
  code yet)**, per the Discovery Convention above. Full audit is in
  the "Confirmed research findings" section (search "Remita — FULL API
  discovery pass"). Unlike every other provider audited so far, Remita
  has no single public developer-docs site — the audit is built from a
  patchwork of an official PDF, official `RemitaNet` GitHub SDK repos
  (for a *different* Billing Gateway product, not confirmed to be the
  same surface this repo needs), a public Postman collection, and
  community integrations, several of which document mutually
  inconsistent base URLs/paths for the same nominal operation. Three
  real open items queued there, all blocking implementation rather
  than optional polish: (1) three source families disagree on the
  base URL/path for RRR generation and status-check — get the current
  values directly from the product owner's own onboarding email rather
  than picking one; (2) two incompatible auth schemes exist (classic
  hash-in-URL vs. header-based `remitaConsumerKey`/`remitaConsumerToken`),
  each with two different hash formulas for generate vs. verify — using
  the wrong one silently fails auth; (3) Remita's confirmation model is
  an **unsigned browser redirect** the merchant must independently
  re-verify via a separate hash-authenticated status call, not an
  inbound signed webhook like this repo's other four providers — a
  real design difference for Task 0/b-3's normalized-webhook mapping,
  not just an implementation detail. Also unresolved: the actual
  `statuscode` enumeration (only `025`/pending confirmed against a real
  example) and whether currencies beyond NGN are supported at all (no
  currency parameter found in any RRR-generation example examined).
- **a-7. Xixapay** — **discovery/audit done (2026-09-06, doc-only, no
  code yet)**, per the Discovery Convention above. **Existence
  confirmed** — a real Nigerian payments platform with a real,
  structured docs site (`documentation.xixapay.com`), resolving this
  session's original "could not confirm this is a documented, existing
  payment provider" caveat. Full audit is in the "Confirmed research
  findings" section (search "Xixapay — FULL API discovery pass").
  **Two real items queued there, both blocking implementation:**
  (1) authentication needs **three simultaneous credentials** — a
  `Bearer` secret header, a separate `api-key` header, *and* a
  `businessId` field inside every request body — a genuinely different
  shape from this repo's existing single-Bearer-header providers, not
  a drop-in fit for `getProviderKey()`'s current `public`/`secret`
  pair; (2) the documented webhook signature scheme has **no replay
  protection at all** (plain `HMAC-SHA256(payload, secret)`, no
  timestamp/nonce component, unlike Payscribe/DodoPayments/Korapay/
  Paystack all above) — a real provider-side security gap to design
  around, not an implementation bug this repo would be introducing.
  Also flagged there: Xixapay's own official Node.js webhook example
  has two confirmed bugs (re-serializes the parsed body instead of
  using raw bytes; compares signatures with `===` instead of a
  constant-time function) — do not copy that sample as-is if this
  gets implemented in Node. No sandbox/test-mode host is documented
  anywhere — only one base URL exists in every source found, an open
  question for the product owner before assuming test-mode credentials
  behave identically to live ones.
- **a-8. PaymentPoint** — **FULL discovery/audit done (2026-09-06,
  doc-only, no code yet)**, per the Discovery Convention above, except
  every source doc came from the product owner directly rather than
  this session's own web search — six pages total (Authentication,
  Errors, Webhook Documentation, Create Virtual Account, Identity
  Verification, Liveness Check), matching every page in PaymentPoint's
  own sidebar nav. Full audit is in the "Confirmed research findings"
  section (search "PaymentPoint — FULL API discovery pass"). **Real
  items queued there, blocking implementation:** (1) auth needs **two
  simultaneous headers** (`Authorization: Bearer {secret}` + separate
  `api-key` header) **plus a body-level `businessId`** — a three-
  credential shape essentially identical to Xixapay's (a-7 above), not
  a drop-in fit for this repo's existing single-Bearer-header
  `getProviderKey()` pattern; (2) webhook signature scheme is
  `HMAC-SHA256(raw_body, secret)` in a `Paymentpoint-Signature` header
  — a good fit for this repo's existing Korapay-style verifier
  pattern, **but** PaymentPoint's own official Node sample has the
  same two confirmed bugs already flagged in Xixapay's sample
  (re-serializes parsed JSON instead of using raw bytes; uses `===`
  instead of a constant-time comparison) — do not copy it as-is; (3)
  no confirmed documented value exists for a failed/pending
  `transaction_status`, blocking Task 0/b-3's normalized-webhook
  mapping for this provider until a real example or the product owner
  supplies one; (4) no replay protection (no timestamp/nonce) in the
  webhook signature scheme, same provider-side gap already on record
  for Xixapay — mitigated by this repo's existing dedupe key (Task
  0/c-1), not fixable on this repo's side alone; (5) **a live-looking
  Bearer token + api-key pair is exposed directly in PaymentPoint's own
  Authentication and Create Virtual Account doc pages** — not
  reproduced anywhere in this repo; flag for the product owner to
  confirm whether it's real and rotate it if so; (6) the Create
  Virtual Account page's own request-body table has a `businessId`
  example that is a copy-paste duplicate of its `api-key` example —
  use the differently-shaped `businessId` from that same page's actual
  code sample instead, and from the Identity Verification/Liveness
  Check pages, which agree with each other. **Two paid verification
  endpoints also newly audited, not required for payment orchestration
  but real product-owner-visible functionality if this platform ever
  exposes them:** Identity Verification (`POST /api/identity/verify`,
  NIN/BVN lookup, returns biometric photo data — handle per the doc's
  own no-logging instruction) and Liveness Check (`POST
  /api/liveness/check` — **must branch on the nested `is_live` field,
  not the top-level `status`**, since a failed liveness check still
  returns HTTP 200 with `"status":"success"`).
- **a-9. Presmit** — **discovery/audit done (2026-09-06, doc-only, no
  code yet)**, per the Discovery Convention above (found via this
  session's own web search — no product-owner-supplied doc needed
  this time). **Existence confirmed, with a name correction:** the
  real provider is **Prestmit** (with a "t"), not "Presmit" —
  `documentation.prestmit.io`, a real, structured GitBook-style docs
  site with its own `llms.txt` index. Full audit is in the "Confirmed
  research findings" section (search "Prestmit — FULL API discovery
  pass"). **The single biggest finding, flagged before anything
  else: Prestmit is not a bank/card payment processor like this
  repo's other nine providers — it's a gift-card and crypto off-ramp
  trading platform.** Its API has no "charge a customer"/"initiate a
  payin" endpoint at all; the closest analogs are "create a gift-card
  sell/buy trade" and "create a wallet withdrawal." Before any
  provider code is written, the product owner needs to confirm
  whether Task 0's inclusion of Prestmit in the payment-provider list
  was intentional (e.g. gift-card liquidation as one more "payment
  method" this platform offers) or a mix-up with a different,
  bank/card-based provider — this is a real scope question, not an
  implementation detail. **Real items also queued there, blocking
  implementation regardless of how the scope question resolves:**
  (1) **three mutually inconsistent base-URL sources found in
  Prestmit's own docs** — the Authentication guide states one
  sandbox/live pair, the OpenAPI spec's `servers` block states a
  second, and that same OpenAPI spec's own prose description states a
  *third* — a real, confirmed conflict, same class of problem already
  on record for Remita (a-6), not something to guess at; (2) auth is
  **HMAC-SHA256 over `API_KEY + ":" + JSON.stringify(body)`**, keyed
  with a separate `API_SECRET`, sent as an `API-Hash` header alongside
  a plain `API-KEY` header — a genuinely different signing shape from
  every one of this repo's other nine providers (none of which sign
  the *request*, only inbound webhooks); (3) webhook signature
  verification — `x-prestmit-signature` header, HMAC-SHA256 over the
  raw body, base64-encoded (not hex, unlike every other provider in
  this file) — and **Prestmit's own official webhook-verification
  sample has the identical bug already flagged for Xixapay and
  PaymentPoint**: it hashes `JSON.stringify(payload)` (the
  re-serialized parsed body) instead of the original raw bytes — a
  third independent confirmation across three different providers
  that provider-supplied Node samples are not safe to copy as-is in
  this repo.
- **a-10. `telcos.opik.net`** — not started. Even though the product
  owner owns this endpoint, whoever builds against it still needs its
  actual request/response contract, auth scheme, and a real answer on
  uptime/reliability posture before real customer money depends on
  it — same rigor as any third-party provider, not skipped because
  it's personally operated.

**b. Orchestration / routing layer design** — not started.
- b-1. Decide the routing-rule model: this repo already has a
  `ROUTING_RULES` concept (used for `/payout` provider defaulting) —
  extend it, or replace it, once (a) is far enough along to know what
  routing actually needs (currency, country, payment method, cost,
  provider uptime)?
- b-2. **Resolved this session — self-describing, not opaque, and not
  something the calling business ever supplies or sees.** Given the
  no-DB constraint (see above), there is nowhere else "which provider
  actually handled this transaction" can live except the reference
  string itself — no database row to look it up in, no admin
  dashboard maintaining that mapping. So the reference this repo
  generates at checkout time has to carry that fact internally.
  Concretely: `{TENANT}-{PROVIDER_CODE}-{timestamp}-{random}`, where
  `PROVIDER_CODE` is a short internal-only code this repo assigns
  per provider (e.g. `K1` for Korapay, `P1` for Paystack) — never the
  provider's real name, and never a field the business is asked to
  pass in at checkout or sees labeled "provider" anywhere in this
  platform's own API. The business only ever sends the parameters
  that describe the transaction (amount, currency, country, payment
  method, etc.); this repo's own routing logic (b-1) picks the
  provider from those parameters and stamps the code into the
  reference it hands back — the business never names a provider
  going in, and never sees one coming out. To the business, this
  platform is the only provider that exists. `TENANT` keeps Task 41's
  existing multi-app fanout prefix (e.g. `MAVW`) working unchanged —
  the two segments answer two different questions ("which downstream
  app owns this" vs. "which underlying provider handled it") and
  don't interfere with each other. Old references issued before this
  scheme existed simply won't parse a provider code back out — that's
  expected, not a bug, and any code reading the reference should treat
  a failed parse as "provider unknown," not throw.
- b-3. **Resolved this session — a normalized envelope, not raw
  passthrough.** Whatever consumes this repo's forwarded webhooks
  (Mavins-web, the new home page, or any future business integrating
  directly) receives one consistent shape regardless of which of the
  ten providers actually handled the transaction:
  `{ id, type, reference, amount, currency, status, occurred_at,
  metadata }`, where `type` is one of a fixed, provider-agnostic set
  (`payment.succeeded`, `payment.failed`, `payment.pending`,
  `payout.succeeded`, `payout.failed`, `refund.succeeded`,
  `refund.failed`) and `status` is one of `succeeded` / `failed` /
  `pending`. Each provider's raw webhook gets mapped into this shape
  before anything is forwarded — the raw provider payload itself is
  never forwarded downstream, even with provider names redacted,
  because field names and nesting differ enough between providers
  (Korapay vs. Paystack vs. Juicyway) that the shape alone would
  fingerprint which one handled it, which is exactly the leak this
  was meant to close. `metadata` is a pass-through bag for whatever
  the business itself supplied at checkout (their own order id, etc.)
  — this repo never invents fields here, only relays what it was
  given. One per-provider mapping has to be written for each of the
  ten providers as each is integrated (a) — Paystack/Korapay/Juicyway
  are already well enough understood from Tasks 3-5 to write theirs
  now; Payscribe's mapping waits on Task 6 confirming its real event
  shape first; DodoPayments/Flutterwave/Remita/Xixapay/PaymentPoint/
  Presmit/telcos.opik.net all wait on their own a-4..a-10 discovery
  passes before a mapping can be written responsibly.

**c. No-database operational risk** — flagged, not blocking, but
needs a real, explicit answer before this carries production traffic:
- c-1. **Resolved this session, with one honest caveat left open.**
  Dedupe key is `provider:event:reference`, held in an in-memory store
  — the same pattern Task 41's `webhookGateway.js` already uses for
  its own event log, generalized here to cover every provider's
  webhook handler, not just Korapay's. A redelivered webhook with a
  key already seen is acknowledged 200 but not re-forwarded. **The
  caveat, stated plainly and not glossed over:** this only dedupes
  within one running process. It does not survive a restart/redeploy,
  and if this backend ever runs as more than one instance behind a
  load balancer, a redelivery landing on a different instance than
  the original won't be caught — the same limitation `webhookGateway.js`
  already documents for its own store, now true platform-wide rather
  than just for the Korapay fanout path. That's a real, still-open
  decision if and when this runs multi-instance: either pin webhook
  traffic to a single instance, or move the dedupe store to something
  that survives a restart and is shared across instances (a small
  external cache, e.g. Redis — a cache used only for short-lived
  dedupe keys, not a system of record, so it doesn't reopen the "no
  database" constraint above). Single-instance today, so not urgent,
  but worth deciding before scaling instances rather than after a
  duplicate payout slips through.
- c-2. **Reconciliation — partially answered (see b-2):** the product
  owner will reconcile manually via each provider's own dashboard,
  correlated by reference, rather than this repo maintaining any
  transaction record of its own. Still open: whether this is
  sufficient at higher transaction volume, and whether any of the ten
  providers' own dashboards are themselves adequate for dispute
  resolution (not yet checked per-provider).
- c-3. **Audit trail:** several of these providers move real money.
  Confirm directly with the product owner whether relying on each
  provider's own dashboard as the only record (no database, no
  dashboard of this repo's own) is compatible with whatever
  compliance/audit obligations apply to the business, before this
  becomes expensive to walk back later.

**d. Customer-facing surface** — not started. **Confirmed by the
product owner: no separate merchant/admin dashboard is in scope** —
see b-2. **Reversed, 2026-09-06 — see Task 46**: the product owner has
since decided a full merchant/admin dashboard is in scope after all,
back to being a database-backed feature. What remains in scope,
unchanged: customer-facing surface, plus one owner-only admin route
(d-3 below), now joined by the dashboard covered in Task 46.
- d-1. Dynamic white-label checkout page: no config schema yet
  (branding, which providers/currencies/methods are enabled per
  merchant, etc.).
- d-2. Home page: not yet scoped beyond "users land here and reach
  checkout."
- d-3. Admin route, **explicitly not exposed to end users** — at
  minimum lets the platform owner change the dynamic default provider
  (Korapay initially). Not yet scoped beyond that: how it's
  authenticated, whether it covers anything besides default-provider
  selection, and how "not exposed to end users" is actually enforced
  (separate route namespace kept out of any public router table? its
  own auth gate distinct from `requireInternalApiKey`? not decided).

**e. Testability / go-live philosophy, stated directly by the product
owner this session:** the intent is for this system to be fully
built and tested end-to-end using placeholder/test API keys in each
session's own sandbox, so that going live is reduced to the platform
owner inserting real API keys as environment variables on Render —
no further code changes needed at that point. Implications for every
future implementation session:
- Every provider integration needs to be exercised against that
  provider's own sandbox/test-mode credentials (not just written and
  assumed correct) before being considered done.
- Code must read all credentials via environment variables (this repo
  already does this via `getProviderKey()` for existing providers —
  continue that pattern for every new provider, don't hardcode
  anything provider-specific).
- "Fully tested with placeholder keys" means test-mode keys for each
  provider's own sandbox, not fabricated dummy values with no real
  provider behind them — a session that can't get real sandbox
  credentials for a provider should ask the product owner (same
  discovery-convention rule as above), not invent a fake response
  shape and call it tested.

**Not yet done, this session, deliberately — documentation only.** No
provider code, no routing code, no checkout page, no home page, no
admin route. This task exists so the next session knows the full
shape of the goal and exactly where to start (`a-1-i-X`), instead of
re-deriving scope from a standing start.

---

## Task 45 — Reseller API (`telcos.opik.net`) endpoint audit; Supabase-backed dashboard, transaction tracking/suspension, and public/secret key model decided this session [ ]

**Scope note, read first:** `telcos.opik.net` is Task 0/a-10 — the
product owner's own personal endpoint (global VTU/airtime-data
services), listed there as "not started." This task is that discovery
pass, plus a scope decision the product owner made this session about
where that product is headed. **This does not, by itself, reopen
Task 0/c ("no-database operational risk") or Task 0/d ("no separate
merchant/admin dashboard is in scope") for `B-Pay-backend`'s own
payout-routing role** — those were confirmed decisions about this
repo acting as a stateless orchestration layer across ten payment
providers, and nothing here changes that. What changes is the product
owner's plan for the Reseller/VTU product itself, which is a separate
running service this repo only references as a future integration
target. **Flagged explicitly for the product owner to confirm:** if
the intent was actually to reopen Task 0/c-d for `B-Pay-backend`
itself, say so directly — the instruction that triggered this task
("link this payment infrastructure with Supabase... build a full
dashboard...") is genuinely readable either way, and this write-up
assumes the narrower reading rather than silently assuming the
broader, more consequential one.

**a. Endpoint audit — done this session.** Captured from the live
Swagger UI at `https://telcos.opik.net/api/v1/docs` (all 10 operations
were already expanded in the source capture, so this is transcribed
from the real server, not inferred): `POST /auth/register`,
`POST /auth/login`, `GET /plans`, `GET /wallet`,
`POST /wallet/deposit`, `POST /purchase/data`,
`POST /purchase/airtime`, `GET /transactions`, `GET /webhooks`,
`POST /webhooks`. Base URL: `https://telco.opik.net/api/v1`. Full
field-level contract, a modular OpenAPI 3.0 spec (one file per module,
so a new country's VTU rail never requires touching an unrelated
file), and human-readable guides now live in this repo under `docs/`
— see `docs/README.md` for the index and update instructions, and
`docs/guides/09-conventions-and-open-items.md` for the consolidated
list of everything the Swagger capture didn't show (exact auth header,
full error shape, full `transactions[].type`/`status` enums, full
webhook `events` set, insufficient-balance behavior — 10 items total,
each still needing direct confirmation against the live server before
a client integration relies on it). **This resolves Task 0/a-10's
"not started" as far as request/response contract goes** — the
uptime/reliability posture half of that item is still genuinely open.

**b. Supabase integration — decided this session, not yet built.**
Product owner direction: link the Reseller/VTU product to Supabase so
that every transaction is tracked centrally (not left to
per-business/per-provider reconciliation) and so that a business (or a
specific card/transaction) can be suspended. Proposed shape, offered
as a starting point for whichever session actually builds this — not
yet confirmed with the product owner at the field level:
- `businesses` — one row per registered business (mirrors today's
  `POST /auth/register`), plus a `status` column
  (`active`/`suspended`) that every authenticated request checks
  before doing anything else — fail closed, same posture this repo's
  own `requireInternalApiKey` already uses for an unconfigured key.
- `api_keys` — see (d) below; separate table rather than columns on
  `businesses` so a business can hold multiple keys (e.g. live + test)
  and so a compromised key can be revoked without touching the
  business row itself.
- `transactions` — mirrors the existing `GET /transactions` response
  shape (`id`, `type`, `amount`, `status`, `reference`, `created_at`)
  plus a `business_id` foreign key; this is the row the dashboard (c)
  reads from, and the row a `POST /purchase/*` call writes at request
  time rather than only relying on the provider's own record.
- `webhook_endpoints` and `webhook_deliveries` — separates "what a
  business registered" from "what was actually sent and whether it
  succeeded," so delivery failures are queryable instead of silent.

  Genuinely open, not addressed above: Row-Level Security policy
  design (which of these tables a business's own dashboard session can
  read directly vs. only through the API), whether `suspend` blocks at
  the auth-check layer or per-endpoint, and migration of any
  transaction history that predates this table existing.

**c. Full dashboard — decided this session, not yet built.** Product
owner direction: a Korapay-style dashboard — businesses log in and see
their own transactions/wallet/webhooks; the product owner has an
admin view that can see and suspend any business. This is new scope,
not previously mentioned anywhere in this file for the Reseller/VTU
product. Genuinely open: dashboard auth (a business's dashboard login
is not the same credential as their API secret key — conflating the
two means a leaked API key also grants dashboard access, which is
worse than either leak alone), hosting/framework choice, and whether
"admin" is the product owner alone or a role multiple people can hold.

**d. Public/secret key model — decided this session, not yet built.**
Move off the single `api_key` returned by `POST /auth/register` today
(see `docs/guides/02-authentication.md`) onto an industry-standard
public/secret pair — `pk_live_.../pk_test_...` (safe client-side,
identifies the business) and `sk_live_.../sk_test_...` (server-side
only, shown once at generation, hashed at rest — never store or
re-display the raw secret after issuance, same principle as never
logging a provider's raw API key elsewhere in this codebase). Keys are
generated/rotated/revoked from the dashboard (c), not only issued once
at registration as today. Genuinely open: whether existing `api_key`
holders get a forced migration or a grace period running both schemes,
and the exact rotation UX (does rotating immediately invalidate the
old secret, or is there an overlap window).

**e. White-label — decided this session, not yet built.** Each
business gets a brandable surface (at minimum a checkout/deposit page
matching their own name/logo/colors, per the product owner's "custom
white label" direction) rather than a page carrying this platform's
own branding. No config schema yet — genuinely open, same category of
gap as Task 0/d-1's dynamic white-label checkout page for
`B-Pay-backend`'s own separate customer-facing surface; the two should
probably share a config shape if one business could plausibly sit on
both products, but that hasn't been confirmed with the product owner
either.

**Not yet done, this session, deliberately — documentation and
decision-recording only,** matching this repo's own established
pattern (see Task 0's closing note). No Supabase project created, no
schema migrated, no dashboard code, no key-issuance code. This task
exists so the next session that picks up the Reseller/VTU product
knows the full shape of where it's headed and exactly which sub-item
is unconfirmed, instead of re-deriving scope from a standing start —
and so the scope-boundary question in this task's own opening note
gets the product owner's direct answer before any session assumes
either reading of it.

---

## Task 46 — Product owner reverses the no-database / no-dashboard constraint for `B-Pay-backend` itself; full admin dashboard + Swagger UI decided this session [ ]

**Scope note, read first:** this is the direct answer to the question
Task 45 explicitly left open ("if the intent was actually to reopen
Task 0/c-d for `B-Pay-backend` itself, say so directly"). The product
owner has now said so directly, this session, and picked the broader
reading: this repo, not just the separate Reseller/VTU product, is
getting a database and a full dashboard.

**a. The reversal, stated directly by the product owner this
session.** Task 0's original constraint — no database, no separate
merchant/admin dashboard, stateless orchestration only — is **no
longer the design for this repo.** Reason given: the product owner
wants admin roles that can (1) turn specific countries on or off for
a specific business, and (2) activate/deactivate individual cards,
neither of which is possible without somewhere to persist
business-level and card-level state. Both of Task 0's `c` ("no-
database operational risk") and `d` ("no separate merchant/admin
dashboard is in scope") are reopened for this repo specifically — see
the inline notes now left at Task 0's own constraint paragraph and at
Task 0/d above.

**b. Dashboard scope, as described by the product owner this
session:**
- Businesses log in and see their own activity — same baseline as the
  Korapay-style dashboard already decided for the separate Reseller
  product in Task 45/c, so the two can likely share a design
  vocabulary even though they're different products.
- **Admin role(s)** — not yet defined how many people can hold this,
  same open question as Task 45/c flagged for the Reseller dashboard
  — can:
  - Enable or disable which countries a given business is permitted
    to transact in (a per-business allowlist, not a single global
    country list).
  - Activate or deactivate individual cards.
  - (Implied, not yet confirmed directly: see and suspend a business
    outright, the same capability already decided for the Reseller
    product's admin view in Task 45/c — worth confirming this is
    wanted here too rather than assuming it carries over.)
- **Stated advantage over Korapay's own dashboard, per the product
  owner:** because this repo is Task 0's canonical, ten-provider
  consolidation layer rather than a single provider, one dashboard
  here can surface a business's activity across every underlying
  provider in one place — something Korapay's own dashboard, scoped
  to Korapay alone, structurally cannot do. This session did not
  independently verify Korapay's current dashboard feature set to
  confirm the comparison; it's recorded here as the product owner's
  stated rationale, not an audited fact.

**c. Swagger UI, decided this session.** This backend's own API
(`/pay`, `/payout`, `/verify`, `/banks`, and the per-provider webhook
routes in `routes.js`/`webhookGateway.js`) gets a Swagger/OpenAPI UI
for documentation. **Distinct from the existing `docs/openapi`
material** — that was written this session's earlier work (Task 45a)
for the separate `telcos.opik.net` Reseller API, a different service
with a different base URL and endpoint set. This backend needs its
own spec written from `routes.js`'s real routes, not copied from the
Reseller spec.

**d. Proposed starting schema — offered as a starting point, not yet
confirmed with the product owner at the field level, same caveat Task
45/b already carries for the Reseller product's own schema:**
- `businesses` — one row per business using this platform (mirrors
  whatever this repo's own business-onboarding flow turns out to be —
  not yet designed, since businesses were previously stateless callers
  of a no-DB API).
- `admins` — platform-owner-side accounts distinct from `businesses`,
  since an admin role and a business login are different trust levels
  (same separation-of-credentials principle Task 45/c already flagged
  for not conflating a business's dashboard login with its API
  secret key).
- `business_country_permissions` — one row per
  `(business_id, country_code)`, an allowlist an admin toggles rather
  than a single boolean column, so a business can hold a growing or
  shrinking set of countries over time without a schema change.
- `cards` — one row per card known to this platform, with a `status`
  column (`active`/`deactivated`) an admin can flip; foreign-keyed to
  whichever `businesses` row the card belongs to.

  Genuinely open, not addressed above: whether `cards` here means
  cards this platform issues itself or cards this platform has seen
  in transactions routed through it (a very different data-ownership
  and compliance posture — PCI scope changes substantially depending
  on which one this is); Row-Level Security / access-control design
  once a database engine is chosen; and how existing in-flight
  integrations (Korapay/Paystack/Juicyway/Payscribe, all built under
  the no-DB constraint) migrate onto business records that didn't
  exist when they were written.

**e. Database engine and hosting — RESOLVED in Task 47 below:
Supabase, confirmed directly by the product owner.** (Left here for
history; see Task 47/b for the decision itself and what's still open
about it — namely, whether it's the same Supabase project as Task
45's Reseller product or a separate one, which Task 47 does not yet
resolve.)

**Not yet done, this session, deliberately — documentation and
decision-recording only,** matching Task 0 and Task 45's own closing
notes. No database provisioned, no schema migrated, no dashboard code,
no Swagger route added, no admin-role code. This task exists so the
next session that picks this up has the full reversal on record, the
proposed schema as a starting point, and the genuinely-open questions
(engine choice, admin-role headcount, card data-ownership meaning)
called out explicitly rather than assumed one way or the other.

**On dumping/migrating a real schema once one exists:** there is
currently **no live database anywhere in this project** to dump —
Task 0's constraint meant none was ever provisioned. Once the product
owner (or a future session) stands one up and the schema above (or a
confirmed version of it) is actually created in it, the standard way
to capture that schema for this repo's own migrations directory is a
schema-only `pg_dump`, run from wherever the database is reachable —
see the product owner's own Termux environment note under "Patch
Handoff Convention" below for how commands are run from that device.
That dump belongs under a new `db/migrations/` (or similar) directory
in this repo, committed the same way any other change here is — via
a patch, per the convention immediately below, not applied directly.

---

## Task 47 — Product owner states the platform-identity vision (fully hidden underlying rail) and resolves Task 46/e's database engine question [ ]

**Scope note, read first:** documentation/decision-record only, same
discipline as Task 0/Task 45/Task 46's own closing notes. No code, no
schema, no dashboard UI this session.

**a. The vision, stated directly by the product owner this session.**
This backend is not meant to read to anyone as a wrapper around
Korapay/Paystack/JuicyWay/Payscribe/etc. — it is meant to **be** the
payment provider a business or its customers interact with, full
stop. This goes further than the white-label checkout page already
decided at Task 0/d-1 and Task 45/e: those cover a business's own
branding on top of this platform; this is about **this platform's own
identity being the only one visible**, with which underlying rail
actually processed a given transaction, and even which payment method
was used, treated as an internal implementation detail rather than
something surfaced to the end user. Not yet addressed, and genuinely
open for whichever session picks this up: how this interacts with any
provider-specific disclosure a payment method may legally require at
checkout (e.g., redirect-based methods that briefly show a provider's
own domain) — flagged here, not resolved.

**b. Database engine and hosting — resolved.** Supabase, confirmed
directly by the product owner this session, closing the question Task
46/e left open. Still genuinely open, not addressed this session:
whether this backend's dashboard tables live in the same Supabase
project as Task 45's Reseller/VTU product or a separate one — Task
46/e's original risk-profile note (this backend moves real money
across ten providers; the Reseller product is airtime/VTU) still
applies and hasn't been weighed against the one-project convenience
argument.

**c. Dashboard parity target — reference material produced this
session, not new scope beyond Task 46/b's own admin-capability list.**
Alongside the admin roles already decided (per-business country
allowlist toggling, per-card activate/deactivate), the dashboard
should reach feature parity with what a Korapay-style dashboard
offers a business day to day:
- Transaction history
- Settlement / payout reports
- Balance overview
- Webhook delivery logs
- Refund / dispute handling
- API key / access management

This list is a target, not a schema — Task 46/d's proposed
`businesses`/`admins`/`business_country_permissions`/`cards` tables
are still the only schema on record, and none of the above has been
mapped onto concrete tables or endpoints yet.

**d. Interaction with Task 46/d's proposed schema — flagged, not
resolved.** This session's (a) (hiding which underlying rail and
payment method handled a transaction) implies transaction-level
records will need to carry that provider/method detail internally
while never exposing it through any dashboard view or API response a
business or its customers can see — a filtering rule the proposed
schema doesn't yet account for, since it predates this session's
identity-concealment direction. Worth resolving before any dashboard
read endpoint is built, not before the schema is created.

**Not yet done, this session, deliberately:** no schema change, no
Supabase project provisioned, no dashboard code, no filtering logic
for hiding provider/method detail. This task exists so the next
session has the platform-identity vision and the resolved database
engine on record, plus the two genuinely-open questions in (a) and
(d) called out explicitly.

---

## Task 48 — Gift cards confirmed in scope; full public-facing service/rail catalog assembled from this file's own existing provider research [ ]

**Scope note, read first:** documentation/decision-record only, same
discipline as Tasks 0/45/46/47. No code, no schema, no new provider
files this session. Everything catalogued below was already audited
elsewhere in this file — this task's job is only to pull it into one
place as the public-facing vision, per the product owner's direction.

**a. Gift cards — confirmed in scope, resolves Task 0/a-9.** Task
0/a-9 flagged an open question: was Prestmit's inclusion in this
repo's ten-provider list intentional, or a mix-up with a different
kind of provider (Prestmit is a gift-card/crypto off-ramp, not a
bank/card processor like the other nine)? **The product owner has now
confirmed, directly, that it was intentional** — gift-card services
are a real, wanted part of this platform's offering. Per Task 47/a's
identity-concealment direction, this is presented publicly as
**"Gift Cards"** — Prestmit itself is never named to end users or
businesses, same treatment as every other underlying rail.

**b. Consolidated public service list — the vision doc, current
state.** Bringing together every decision on record across Tasks
0/45/46/47 and this task:
- Mobile data, airtime, and eSIM reselling — presented via
  `telcos.opik.net`, not as individual underlying providers (Lizzysub/
  Zendit/Accragh stay internal).
- Reseller wallet management (balances, withdrawals).
- White-label branded mobile apps (per-reseller APKs).
- Payment collection (card, bank transfer, mobile money, USSD,
  virtual accounts) — native, provider-agnostic.
- Payouts/disbursements — native, **not tied to any one underlying
  provider** (per the product owner's own correction this session).
- Gift cards — native, per (a) above.
- Card issuance — **planned**, to be built on an underlying provider
  but presented as this platform's own.
- Cross-border settlement — offered today, since the underlying
  providers already handle it fully; presented as native.
- KYC/KYB identity verification, merchant/admin dashboard, settlement/
  balance reporting — **planned** (Task 46/47).

**c. Country footprint — consolidated, not new research.** Derived
directly from this file's own `CONFIRMED_PROVIDER_CURRENCIES`
(`utils/helpers.js`) plus the Task 8d Paystack XOF finding: **Nigeria
(NGN), Ghana (GHS), Kenya (KES), South Africa (ZAR), Cameroon (XAF),
Côte d'Ivoire (XOF), Egypt (EGP), Tanzania (TZS), plus USD
cross-border** — the union of Korapay's and Paystack's own confirmed
lists. **Explicitly excluded, not silently folded in:** JuicyWay and
Prestmit's own country/currency coverage remain unconfirmed per their
own research sections above — this footprint will grow once those are
resolved, and once Remita/Flutterwave are actually implemented (their
own geography is cataloged in (d)/(e) below but neither has a
provider file yet).

**d. Remita's rails, as already audited above — summarized for the
service catalog, not re-researched.** Two structurally separate
surfaces exist (see Remita's own research section): **Surface
1, the classic RRR Payment Gateway** — card, bank transfer, USSD, or
in-branch settlement of a merchant-generated Retrieval Reference —
is the one that fits this repo's existing single-call orchestration
shape and is the one Task 10's routing table should target. **Surface
2, the Billing Gateway**, is a different biller-payment product
(school/church/government MDA bill payment) with its own credential
pair — genuinely a separate scope decision, not yet made, on whether
this platform wants biller-payments as a distinct offering. Still
blocking actual implementation: the three-way base-URL/path
inconsistency and the two incompatible auth schemes already flagged
in Remita's own research section — unresolved here, still needs the
product owner's real onboarding material.

**e. Flutterwave's rails, as already audited above — summarized for
the service catalog, not re-researched.** Once a provider file exists:
card charges (v4 requires field-level AES-256-GCM encryption of card
data), bank transfer, mobile money, USSD, wallet, and virtual accounts
(confirmed as their own separate resource family, not a `charge`
payment-method value). Payouts (`/direct-transfers`) cover domestic
NGN with a minimal bank-account payload, and international EUR/GBP/
USD/ZAR with full recipient KYC (and, for EUR/GBP/USD, sender KYC
too) — plus scheduled/deferred payout timing, and stablecoin-
denominated settlement (`USDC`/`USDT`/`RLUSD` listed as ordinary
currency codes). **Still the primary blocker, unresolved here:** the
v3-vs-v4 version choice flagged in Flutterwave's own research section
above — no implementation should start until the product owner picks
one, for the reasons already on record there (v3 matches this repo's
existing bearer-key pattern; v4 is the actively-promoted default but
needs a first-of-its-kind token-refresh manager under this repo's own
no-persistent-state constraint).

**Not yet done, this session, deliberately:** no schema change, no
provider files created for Remita or Flutterwave, no gift-card
endpoint wired, no country-footprint field added to any API response.
This task exists so the next session has the full public-facing
service vision, the confirmed country list, and both providers' rail
catalogs in one place — with the genuinely-open items (Remita's
auth/base-URL ambiguity, Flutterwave's v3-vs-v4 choice, JuicyWay/
Prestmit geography) called out explicitly rather than assumed away.

---

## Task 49 — Product owner resolves JuicyWay's stablecoin question and Remita's base-URL ambiguity [ ] (a's currency-list edit done; a's amount-unit rule RESEARCHED+CITED, not yet implemented; b still open)

**Scope note, read first:** this task was originally written as a
decision-record only, per this file's own established pattern (Task
0/45/46/47/48) — no code, no `CONFIRMED_PROVIDER_CURRENCIES` edit, no
`providers/remita.js`, that session. **Update (2026-09-07):** the
`CONFIRMED_PROVIDER_CURRENCIES` edit called out as "not yet done"
below has now been made — see the "Newest note" at the top of this
file and part **a**'s own note below for what changed and what
deliberately didn't.

**a. JuicyWay — stablecoin support confirmed.** Task 0/a-4's own "Three
different currency lists" finding above left this genuinely open
between three JuicyWay doc pages that disagreed with each other. The
product owner has now confirmed directly that JuicyWay does support
stablecoins — resolving the conflict in favor of `payments/initialize-
payment.md`'s and `.../cards.md`'s parameter-docs list (`NGN, USD, CAD,
USDT, USDC`) over `overview.md`'s narrower `NGN, CAD` and over the same
cards.md page's own contradictory 422-error text. **Done (2026-09-07):**
this list is now in `CONFIRMED_PROVIDER_CURRENCIES.juicyway` in
`utils/helpers.js`. **Amount-unit rule — researched and cited
(2026-09-07), implementation deliberately deferred:**
`docs.juicyway.com/payments/initialize-payment` documents `amount`
under "Universal Parameters — required for all payment
initializations regardless of the payment method" as "Payment amount
in minor units (e.g., cents, kobo) ... Example: 10000 = $100.00 USD"
— subunit, ×100, stated to apply across every supported currency
(`NGN, USD, CAD, USDT, USDC`) including the stablecoins, since it's
listed as universal rather than per-method; no stablecoin-specific
override found on that page or its linked method-specific guides. This
is a real citation, not a guess — same bar as the confirmed
Korapay/Paystack rules. **A blanket "use subunits/kobo for every
provider" rule was considered and explicitly rejected** instead of
applied here: Korapay is confirmed (Task 7,
developers.korapay.com/docs/checkout-redirect) to want *base* units
with no multiplier, so a blanket subunit rule would silently
100x-overcharge every Korapay transaction — the per-provider,
per-source-confirmed approach stays the rule. **Not yet built, per
direct product-owner instruction this session (documentation only):**
`getAmountFormat`'s `juicyway` case in `utils/helpers.js` still throws,
and `providers/juicyway.js` still sends `data.amount` raw with no
`convertAmountForProvider()` call. A draft of both changes was written
and verified in-session (`node --check` clean; a throwaway sanity
check confirmed `getAmountFormat('juicyway', 'USD')` would return
`{ unit: 'subunit', multiplier: 100 }`, and korapay/paystack stayed
unchanged) but then reverted rather than committed — `origin/main`'s
actual runtime behavior is unchanged by this session. **Concrete next
step, fully unblocked:** re-apply that same change — add the
`juicyway` case to `getAmountFormat` per the citation above, and wire
`convertAmountForProvider(data.amount, 'juicyway', data.currency)`
into `providers/juicyway.js`'s `processPayment`, matching the existing
`paystack.js`/`korapay.js` pattern exactly. Keep it scoped to only the
amount-unit fix — don't bundle in the separate, already-tracked
endpoint-path (`/v1/charges` vs. the confirmed-correct
`/payment-sessions`), auth-header, or payload-shape bugs under Task
45a/45b; those remain their own leaf.

**b. Remita — base URL supplied directly by the product owner:
`https://api.remita.net/`.** Remita's own research section above found
three mutually-inconsistent host/path families across official and
community sources (`remitademo.net`+`login.remita.net`, `demo.remita.net`
+`login.remita.net`, and `www.remitademo.net`'s older `.reg`-style
paths) and explicitly deferred to the product owner's own onboarding
material rather than guessing among them. **`api.remita.net` matches
none of the three** — a fourth answer, not a confirmation of any
previously-documented candidate. Treated as authoritative per this
file's own stated resolution path for that conflict (get it from the
product owner directly). **Genuinely still open, not resolved by this
note:** a bare host isn't enough to implement against — the existing
research shows RRR *generation* and *status-checking* have
historically lived on different paths, sometimes different hosts
entirely, even within a single official source, so a future
implementation session still needs (i) the actual endpoint paths for
both operations under this host, and (ii) which of the two documented
auth schemes (classic hash-embedded-in-URL/body vs. newer
`remitaConsumerKey`/`remitaConsumerToken` header) this host expects —
neither was supplied alongside the base URL.

**Not yet done, this session, deliberately:** no
`CONFIRMED_PROVIDER_CURRENCIES` edit, no `providers/remita.js`, no
sandbox call made against `api.remita.net` to verify it responds as
expected. This task exists so the next session has both resolutions on
record, plus the specific follow-up questions (Remita's actual paths
and auth scheme under the new host) called out explicitly — the
JuicyWay amount-unit follow-up question referenced here in earlier
sessions is now researched and cited under part **a** above, still
needs implementing, not still needing research. **Payscribe, checked
again this session:** still no public API reference site found (same
result as every prior session's search) — stays blocked, not resolved
by assumption.

---

## Task 50 — Real Remita public API docs supplied; corrects prior base-URL research, reveals three parallel API generations [ ]

**Scope note, read first:** decision-record only, per this file's own
established pattern. No code, no `providers/remita.js` created this
session — this is documentation correction, not implementation.

**a. Source.** The product owner supplied a Postman "public
documentation" page snapshot, hosted at `api.remita.net` (this is
where a Postman-generated docs page lives, not a raw API base URL
itself — a distinction worth keeping straight, since Task 49 recorded
`api.remita.net` as "the base URL," which this task now refines: it is
the docs host, and the doc reveals several *different* actual API base
URLs underneath it, none of which is `api.remita.net` itself).

**b. Three parallel API generations, plus a fourth standalone
scheme — corrects this file's prior three-way base-URL guess
entirely.** The prior research (Remita's own research section above,
and Task 49/b) treated this as one product with an unresolved base
URL. The real docs describe Remita as mid-migration, running old and
new side by side:
- **New APIs (RemitaConnect)** — sign up separately at RemitaConnect
  for a Public/Secret key pair. Covers **Agency Banking, Collections
  (billers), Vending (airtime/data/electricity/other utilities),
  Funds Transfer, and Verification.** Entirely new surface area not
  previously on this file's radar — **the Vending line is a direct
  overlap with this product's own VTU/reseller business**, worth a
  separate look independent of whichever payment-collection surface
  gets implemented first.
- **First Gen — "Accept Online Payments" (Checkout Solutions)** — see
  (c) below; the one that actually matches this repo's existing
  provider shape.
- **First Gen — Invoice Generation** — the classic RRR flow that all
  of this file's *prior* Remita research (base-URL conflict, two hash
  schemes, `statuscode` ambiguity, unsigned-redirect confirmation
  model) was actually describing. The supplied snapshot captured only
  this section's intro paragraph, not its endpoint detail — so
  **everything already on record about the classic RRR flow's base
  URL and auth scheme is still unresolved**, not answered by this
  task. Do not assume Task 49/(c) below or the Checkout findings apply
  to this surface.
- **A fourth, standalone v3-engine bearer-token scheme** — `POST
  https://demo.remita.net/remita/exapp/api/v1/send/api/uaasvc/uaa/
  token` with a `username`/`password` body, returning an access token
  valid **one hour**. Documented under "Other First Gen Services,"
  without specifying which endpoints actually require it — genuinely
  unresolved which of (First Gen Invoice Generation / something else)
  this token gates.

**c. First Gen — Accept Online Payments — CONFIRMED with a real worked
example, the best fit for this repo's existing shape.**
- `POST https://api-demo.systemspecsng.com/services/connect-gateway/
  api/v1/payment/charge` — **note: the same doc's own curl example
  shows a different path**, `.../payment-engine/payment/charge` — a
  real, confirmed inconsistency within this single doc's own two
  sections, same class of finding as Flutterwave's inverted env-select
  ternary and PaymentPoint's duplicated example values above.
- Auth: a flat `secretKey` header — not either of the two hash schemes
  Remita's classic-flow research guessed at; this surface needs none
  of that complexity.
- Request body: `firstName`, `lastName`, `email`, `phoneNumber`,
  `paymentIdentifier` (merchant-generated reference — maps onto this
  repo's `generateReference()` the same way every other provider
  does), `currency`, `narration`, `amount`, plus an optional `split`
  object (`bearerSubAccountId` + `subAccountIds` with per-account
  `share` values) whose full schema isn't explained anywhere in the
  supplied snapshot.
- Response: `{ status: "00", message: "Approved or Completed
  Successfully.", data: { paymentLink: "https://payment-qa...
  " } }` — **this is a hosted-checkout-link model**, the same
  redirect-based shape already established for Korapay/Paystack in
  this file, not a raw synchronous card charge.
- `GET .../payment/merchant/verify/{{transRef}}`, same `secretKey`
  header, **verifiable by the merchant's own `paymentIdentifier`
  reference** — a real advantage over JuicyWay, whose own confirmed
  gap (above) is that it has no documented way to verify by reference
  alone.
- **Not resolved by the supplied snapshot:** whether `amount` is base
  units or subunits (the one worked example uses `10000` for a "Test
  Transaction" with no stated currency-unit rule — the same class of
  guess-worthy gap Task 9's `getAmountFormat` guard exists to prevent);
  the full `split` schema; and the complete set of `status`/`message`
  values beyond the one success case shown.

**d. Not yet done, this session, deliberately:** no
`providers/remita.js`, no `CONFIRMED_PROVIDER_CURRENCIES.remita` entry,
no sandbox call made against either the Checkout or classic-RRR
surfaces. This task exists so the next session knows **which of
Remita's several surfaces** any future implementation should target
(Accept Online Payments / Checkout Solutions, per (c), is the
recommended first target — closest fit to this repo's existing
provider shape and the only one with a confirmed working example) —
and so it doesn't conflate this task's findings with the still-open
classic-RRR-flow questions already on record from earlier research.

---
## Task 51 — Routing model decided: Juicyway default for international rails, Korapay default for African rails, all overlapping providers are full fallbacks; Payscribe fully removed [ ]

**Scope note, read first:** two things happened this session and they
are tracked separately on purpose — (1) Payscribe removal is **done,
in code**, per (a) below; (2) the new routing model is **decision-
record only** — the table in (b) is what `ROUTING_RULES`/`getProvider`
in `routes.js` must be rewritten to match, not a description of what
the code already does. Do not conflate the two when picking this task
back up.

### a. Payscribe removal — DONE this session (code + docs)

Per direct product-owner instruction. Every reference removed:
- `providers/payscribe.js` — deleted entirely.
- `routes.js` — import removed; `getProvider()` case removed;
  `webhookHandlers.payscribe` stub removed; `ROUTING_RULES.bank_transfer`
  repointed from `'payscribe'` to `'korapay'` (Payscribe was the only
  provider that rule pointed at, so this was a forced change, not yet
  the broader Task 51/b model below); stale comments referencing
  Payscribe updated or removed throughout.
- `utils/helpers.js` — `payscribe` entry removed from the key map, the
  base-URL map, `validateProviderKeys()`'s provider list,
  `getHealthStatus()`'s provider list, and the `getAmountFormat`
  switch's `case 'payscribe'` (that case was shared with `juicyway`;
  `juicyway`'s own still-open amount-unit question, per Task 49/a, is
  untouched by this removal).
- `render.yaml` — `PAYSCRIBE_SECRET_KEY` env entry removed.
- Confirmed via `grep -rin payscribe` across the repo (excluding this
  file's own historical record, which intentionally keeps Payscribe
  mentions as a record of past research/decisions) — zero remaining
  live references in `.js`/`.json`/`.yaml` files.
- **Per the Patch Handoff Convention:** a `git format-patch` file
  covering all of the above was generated and handed to the product
  owner directly this session — not applied or pushed by this session.

### b. New routing model — DECISION RECORDED, NOT YET IMPLEMENTED IN CODE

**Stated directly by the product owner this session:** stop routing by
abstract `action` string alone (Task 10's still-open gap). Instead,
every rail belongs to exactly one of two domains — **international**
or **African** — each with one default provider that is expected to
be able to fully cover that domain by itself, and every other provider
that overlaps the same domain becomes a **full fallback**: not a stub,
not "not yet implemented" — a fallback must actually work end-to-end,
because the product owner's own stated reason for keeping fallbacks
fully built is that **any fallback can be promoted to default at any
time**, on short notice, so it can't be allowed to lag behind the
current default in capability.

**Table format is deliberately modular — read this before editing:**
each rail-domain gets its own table (b-1, b-2 below), and each table's
rows are independent of the other table's rows. A future session
editing, say, Korapay's fallback list for African rails should only
need to touch table b-2 — not b-1, not the capability matrix in (c).
When you edit a table, **add a dated one-line changelog entry under
that same table** (see the empty "Changelog" line under each) rather
than silently overwriting the previous row — this file's own existing
convention (see the "Newest note" stacking pattern in the START HERE
box) is to preserve what changed and when, not just the current state.

#### b-1. International rails

| Field | Value |
|---|---|
| Domain covers | Cross-border collection, international bank transfer/receive (e.g. **ACH** and other non-African transfer rails), international payout/disbursement, international verify-payout, international bank/institution list lookup, stablecoin (USDT/USDC), any currency outside NGN/GHS/KES/ZAR/XAF/XOF/EGP/TZS |
| **Default** | **Juicyway** — default for every capability in this domain (collection, local-transfer/ACH-equivalent, payout, verify payout, bank-list lookup), not collection-only |
| Fallback 1 | Korapay (covers NGN/GHS/KES/ZAR/USD/XAF/XOF/EGP/TZS overlap only — not CAD/USDT/USDC; today only has payout/verify-payout/bank-list built for African rails, so would need those extended to cover non-African currencies/rails before it's a full fallback here) |
| Fallback 2 | Paystack (NGN/GHS/ZAR/KES/USD overlap only; payout/verify-payout/bank-list are still stubs — not a full fallback for those capabilities yet) |
| Fallback 3 | Flutterwave — **not yet implemented** (`providers/flutterwave.js` does not exist; blocked on the v3-vs-v4 version decision, see this file's own Flutterwave research section above) — must be built before it can actually serve as a fallback, not just be listed as one |
| Changelog | 2026-09-07 — table created this session, per product-owner direction (this task). 2026-09-07 (later same day) — domain coverage expanded per product-owner direction: Juicyway is now default for local-transfer/ACH-equivalent, payout, verify-payout, and bank-list lookup within this domain too, not collection-only. Rule stated as "anything not African rails defaults to Juicyway." |

#### b-2. African rails

| Field | Value |
|---|---|
| Domain covers | Local NGN/GHS/KES/ZAR/XAF/XOF/EGP/TZS collection, local bank transfer (**mobile money and traditional bank rails**), African payout/disbursement, African verify-payout, African bank-list lookup |
| **Default** | **Korapay** — default for every capability in this domain (collection, mobile-money/bank transfer, payout, verify payout, bank-list lookup) |
| Fallback 1 | Paystack (collection overlap only — no payout/bank-transfer/bank-list today; would need `processPayout`/`verifyPayout`/`getBanks` built to be a full fallback for those) |
| Fallback 2 | Juicyway (NGN overlap only, collection-side; no African-rails payout/bank-transfer/bank-list support at all) |
| Fallback 3 | Flutterwave — **not yet implemented**, same blocker as b-1 |
| Changelog | 2026-09-07 — table created this session, per product-owner direction (this task). 2026-09-07 (later same day) — domain-covers wording clarified (mobile money + traditional banks named explicitly) per product-owner direction; no default/fallback provider changed here, this domain's boundary was already Korapay-default. |

### c. Capability matrix (cross-cutting reference — do not treat as a third routing table)

This is a read-only summary derived from (b-1)/(b-2) above, kept here
so a session doesn't have to reconstruct it by re-reading both tables.
Edit (b-1)/(b-2) first if something changes; update this matrix to
match afterward, not the other way around.

| Capability | Juicyway | Korapay | Paystack | Flutterwave |
|---|:---:|:---:|:---:|:---:|
| International collection | Default | Fallback | Fallback | Fallback (not implemented) |
| African-rails collection | Fallback | Default | Fallback | Fallback (not implemented) |
| International transfer (ACH-equivalent, etc.) | Default | Fallback (needs non-African rails extended) | Fallback (stub) | Fallback (not implemented) |
| African-rails transfer (mobile money, traditional banks) | Fallback (no African-rails transfer support) | Default | Fallback (stub) | Fallback (not implemented) |
| International payout / disbursement | Default | Fallback (needs extending beyond African rails) | Fallback (stub) | Fallback (not implemented) |
| African-rails payout / disbursement | — (no African-rails payout support) | Default | Fallback (stub) | Fallback (not implemented) |
| International verify payout | Default | Fallback (needs extending) | Fallback (stub) | Fallback (not implemented) |
| African-rails verify payout | — | Default | Fallback (stub) | Fallback (not implemented) |
| International bank-list lookup | Default | Fallback (needs extending) | Fallback (stub) | Fallback (not implemented) |
| African-rails bank-list lookup | — | Default | Fallback (stub) | Fallback (not implemented) |
| Webhook verification | Done | Done | Done | Not implemented |
| Confirmed amount-unit rule | Still unconfirmed (Task 49/a) | Confirmed | Confirmed | Unconfirmed |

### d. Not yet done, this session, deliberately

- `ROUTING_RULES` in `routes.js` has NOT been rewritten to the
  domain-based (international/African) model above — it still routes
  by the older four-action shape (`collect_payment`, `bank_transfer`,
  `payout`, `international`), with only the forced Payscribe→Korapay
  `bank_transfer` change from (a) applied. Rewriting `getProvider()`
  and `ROUTING_RULES` to actually pick "Juicyway-unless-African-rail,
  then Korapay-unless-fallback-requested" is real routing-logic work,
  scoped for next session, not done here.
- Paystack's `processPayout`/`verifyPayout`/`getBanks` and all of
  Flutterwave remain unbuilt. Per (b) above, they cannot honestly be
  called full fallbacks until that changes — flagging this explicitly
  so a future session doesn't assume "fallback" in this table means
  "already works."
- **Newly flagged this update:** per the product-owner direction that
  Juicyway must default for every non-African-rails capability (not
  just collection), `providers/juicyway.js` itself does not yet have
  `processPayout`/`verifyPayout`/`getBanks` equivalents for
  international rails (ACH-equivalent transfer, international payout,
  international verify-payout, international bank-list lookup) — none
  of that exists in the provider file today, only `processPayment` /
  `verifyTransaction` / `verifyWebhookSignature`. Korapay's own
  payout/verify-payout/bank-list methods are also African-rails-scoped
  today (see `providers/korapay.js`) and would need to be confirmed or
  extended before they can genuinely fall back for the international
  domain, per b-1's Fallback 1 note. Both are real implementation gaps
  the capability matrix above now surfaces row-by-row — not resolved
  in this update, which is decision-record only.
- No currency/country logic was added to auto-detect which domain
  (international vs. African) an incoming request belongs to — that
  detection logic is itself unscoped work, needed before the tables
  above can drive real routing decisions.

---

## Task 52 — Implement every gap Task 51's capability matrix flagged: build out Juicyway/Korapay/Paystack/Flutterwave fully so the domain-based routing model is real, not aspirational [ ] (b is X, per the 2026-09-08 session's resolution of a-3 and, with it, all of (a) — see Task Numbering & Workflow Convention above)

**Scope note, read first:** this task exists because Task 51 recorded
a *decision* (Juicyway defaults for every international-rails
capability, Korapay for every African-rails capability, all
overlapping providers are full fallbacks) without the underlying
provider code to back most of it up. Per direct product-owner
instruction this session: **this is payment infrastructure — it needs
to be built completely, not left as a routing table pointing at
providers that can't yet do the thing the table says they default (or
fall back) to.** This task's job is to break that gap into atomic,
assignable leaves using this file's own Task Numbering & Workflow
Convention, not to close the gap itself (no code was written this
session — decision/scoping record only, same as Task 51).

**Exactly one leaf below carries the `X` marker at any time, per the
Workflow Convention.** This session (2026-09-08) closed a-1 (i-iv),
a-2, and now a-3 — all of section (a), JuicyWay, is fully resolved.
See each subsection below for citations and the accompanying code
patches. `X` now moves to **b** (Korapay), to be split into the same
i/zi/zo shape once someone picks it up. Whichever session picks this
up next works ONLY on b until it's solved, then moves `X` to c, then
d, then e — unless the product owner explicitly reprioritizes, in
which case update this line to say so and move `X` accordingly.

### a. Juicyway — build the missing international-rails methods [x]

Today `providers/juicyway.js` only implements `processPayment`,
`verifyTransaction`, and `verifyWebhookSignature`. Task 51 made
Juicyway the *default* for every capability in the international-rails
domain, not just collection — these three sub-leaves are what's
missing to make that true in code, not just on paper.

#### a-1. `processPayout` — international payout/disbursement [x]

**Retrofitted 2026-09-07 into the new six-level split
(a/b/c/d/e → 1/2/3/4 → i/ii/iii → zi/zo → X) — this is the worked
migration example the convention above points to.** Original scope,
unchanged: confirming Juicyway's actual payout/disbursement endpoint
against a primary source (same bar Task 7/49 applied to
Korapay/JuicyWay's other endpoints — no guessing a path or payload
shape), then implementing `processPayout(data)` on the `Juicyway`
class matching the existing `Korapay.processPayout` shape so
`routes.js`'s `/payout` route can dispatch to it once (e) rewires
routing. Currency/amount-unit handling for payouts specifically should
be checked against Task 49/a's existing citation (JuicyWay's
"Universal Parameters" subunit rule) rather than assumed to be the
same rule as collection, since payout and collection are documented
as separate surfaces on other providers (see Korapay's own split
between accept-payments docs and payout-via-api docs, Task 7).

##### a-1-i. Locate and confirm JuicyWay's payout/disbursement endpoint against a primary source [x]

###### a-1-i-zi. Find JuicyWay's own API reference for payout/disbursement [x]

Resolved this session (2026-09-08), reopened after the prior session's
negative result above (kept verbatim for the record instead of
deleted, since it correctly documents *why* nothing was guessed at
the time). This session found the real docs site —
`docs.juicyway.com/llms.txt` lists a full "Payouts" section separate
from "Transfers"/"Beneficiaries" — and confirmed **`POST /payouts`**
(operationId `Payout.Web.Controller.initiate_payout`, 201 on success,
400/422 on error) via the raw OpenAPI spec at
`docs.juicyway.com/reference/payouts/initiate-a-payout.md`, plus a
full worked-example guide at
`docs.juicyway.com/transfers/transfers/initiate-bank-transfer.md`
(both a local-NGN and an international-USD example, both posting to
this same path — see a-1-i-zo for the payload itself).

**Cross-reference, not in this leaf's scope but worth flagging where
found:** the same OpenAPI spec's base `servers: []` (empty) means the
host still isn't confirmed from this document alone, and the sibling
"Execute Bulk Transfer" reference page's curl example hits
`api.spendjuice.com`, not a `juicyway.com` host — a real, citable data
point for Task 45a's still-open "verify JuicyWay's base URL / whether
`/v1/charges` is the right collection path" question, which this leaf
does not attempt to resolve.

###### a-1-i-zo. Confirm the payout payload shape once zi's endpoint is found [x]

Resolved this session. `initiate-bank-transfer.md`'s two worked
examples (request AND response, both local and international) confirm
the shape directly — no field names guessed:

Request: `amount` (integer, minor units — see a-1-iii), `beneficiary`
(object: `id`, `type`), `description` (string, optional, max 200
chars), `destination_currency`, `pin` (string — appears in both worked
examples; not marked `required` in the page's own `<ParamField>` block,
which is itself internally inconsistent/malformed markup on Juicyway's
side, but its presence in every example is a stronger signal than an
ambiguous schema annotation), `reference` (string, must be unique),
`source_currency`, `fee_charged_to` (optional, one of `sender` /
`recipient`, default `sender`).

Response: `data.{beneficiary, beneficiary_type, created_at,
destination_amount, destination_currency, id, source_amount,
source_currency, status, updated_at}` — `status` is documented as
`"pending"` in both worked examples; no documented synchronous
`"failed"` outcome the way Korapay's payout response has one (see
`korapay.js#processPayout`'s own comment on that asymmetry).

**Important shape difference from Korapay, discovered here, not
guessed:** JuicyWay's payout endpoint takes a `beneficiary.id`
referencing a resource created ahead of time via the separate
Beneficiaries API (`docs.juicyway.com/transfers/beneficiaries`) —
there is no raw `bank_code`/`account_number` passthrough field on this
endpoint the way `Korapay.processPayout`'s `destination.bank_account`
has. See a-1-iv below, split off because of this finding.

##### a-1-ii. Implement `processPayout(data)` on the `Juicyway` class per the confirmed shape [x]

Resolved this session — see this session's patch. Implemented against
a-1-i-zo's confirmed shape exactly: takes `data.beneficiary_id` (or
`data.beneficiary?.id`) rather than raw account fields, and fails
loudly (not silently) if no beneficiary id is available, pointing at
a-1-iv rather than trying to synthesize one inline. `pin` is read from
`data.pin` or a new `JUICYWAY_PAYOUT_PIN` env var (following the same
pattern `businessId` uses in this file for a credential that doesn't
fit `getProviderKey()`'s existing public/secret shape) and the call
fails loudly if neither is set, rather than sending a request Juicyway
would reject anyway. Mirrors `Korapay.processPayout`'s logging and
`handleApiCall` wrapping, but does NOT copy Korapay's synchronous
`status === 'failed'` check, since a-1-i-zo found no documented
failure status on this endpoint to check for.

##### a-1-iii. Verify payout-specific amount-unit handling against Task 49/a's subunit citation [x]

Resolved this session. `initiate-bank-transfer.md` states directly,
in its own Request Parameters section: "Transfer amount in minor
units (e.g., cents, kobo)" — the same subunit rule Task 49/a already
cited for collection, confirmed independently here for payout rather
than assumed. `processPayout(data)` does not run `data.amount` through
`convertAmountForProvider()` (unlike `Korapay.processPayout`, which
does) — callers are expected to already pass minor units, matching
`Juicyway.processPayment`'s existing convention in this same file. If
that convention changes for one method it should change for both, as
its own future leaf, not silently diverge between them.

##### a-1-iv. Create-or-resolve a JuicyWay beneficiary from raw account details [x]

**New leaf, split off from a-1-i-zo's finding above (not part of the
original a-1 scope written before this session).** `routes.js`'s
`/payout` route, and every existing caller of `Korapay.processPayout`,
pass raw `bank_code`/`account_number` — JuicyWay's endpoint needs a
pre-created `beneficiary.id` instead (a-1-i-zo). Until this leaf is
solved, `Juicyway.processPayout()` only works for callers that already
happen to hold a JuicyWay beneficiary id, which is not yet anything in
this codebase — so a-1 as a whole is NOT fully wired end-to-end yet
despite a-1-i/ii/iii all being resolved.

**Resolved this session, with one honest caveat carried forward
rather than hidden:**

1. **Field shape — confirmed against a primary source.**
   `docs.juicyway.com/transfers/beneficiaries` (the Beneficiaries
   overview page itself, fetched directly this session) documents the
   required fields for all three beneficiary types in its own
   "Beneficiary Information" section: bank account
   (`account_details.{account_number, account_name, bank_code,
   currency}`), crypto address (`crypto_details.{address, chain,
   currency}`), and Interac (`interac_details.{email, name.first_name,
   name.last_name, phone_number (optional)}`). This is what
   `createBeneficiary()`'s per-type payload building is built against.
2. **Exact endpoint path — NOT confirmed, flagged rather than
   guessed-and-hidden.** The dedicated
   `transfers/beneficiaries/create-beneficiary` sub-page (linked from
   the overview page as "See Create Beneficiary") could not be
   fetched this session — repeated attempts were blocked by the
   fetch tool's own same-session-search-result requirement, and no
   search query surfaced its exact HTTP method/path as a primary-source
   citation (the openapi.json tag list this session already confirmed
   for a-1-i-zi has no `beneficiaries` tag at all, meaning — like
   `transfers/transfers/initiate-bank-transfer.md` turned out to
   actually be `POST /payouts`, not something under `/transfers/...`
   — the real path likely doesn't match the doc site's own URL/folder
   naming). Rather than block this whole leaf on that one unconfirmed
   detail (which would just recreate a-1-i-zi's original stuck state,
   now one level deeper), this follows the exact precedent this same
   file already set for `processPayment()`'s `/v1/charges` — implement
   against the confirmed field shape, mark the path itself with an
   explicit "not confirmed, verify before relying on this outside a
   sandbox smoke test" comment, same wording pattern. **Whoever next
   touches this file should treat `POST /beneficiaries` in
   `createBeneficiary()` as no more trustworthy than
   `processPayment()`'s own long-standing `/v1/charges` placeholder
   — both need the same escalation (JuicyWay support / dashboard) that
   Task 45a already exists to do for the collection side. This does
   NOT close Task 45a's scope; Task 45a should now also cover this
   path once someone picks it up.**
3. **Design decision (documented here per this leaf's own instruction
   to record it): beneficiary creation is a separate, explicit
   `createBeneficiary()` call, NOT auto-invoked inline inside
   `processPayout()` when no `beneficiary_id` is supplied.** Reasoning:
   JuicyWay's own "Transfers Overview" guide documents beneficiary
   creation as its own numbered step 1, ahead of "Initiate Transfer"
   step 2 — treating the two as separate calls matches JuicyWay's own
   documented mental model rather than fighting it. It also avoids
   creating a throwaway beneficiary resource on every retried/failed
   payout attempt (auto-create-inline would do this on every call
   that doesn't already have an id, including retries), and lets a
   caller cache a `beneficiary_id` across repeated payouts to the same
   recipient. Whether JuicyWay's beneficiary list de-dupes server-side
   was NOT confirmed either way this session, but that question no
   longer matters much given this decision — it would only have mattered
   for the auto-create-inline path, which wasn't chosen. Wiring
   `routes.js`'s `/payout` route to call `createBeneficiary()` before
   `processPayout()` when it only has raw account details is (e)'s job
   (routing rewire), not this leaf's.



#### a-2. `verifyPayout` — international verify-payout [x]

Resolved this session (2026-09-08). Endpoint located via
`docs.juicyway.com/llms.txt` ("Get payout details", sibling to a-1's
confirmed `POST /payouts` in the same reference tree, immediately
after "Generate a payout receipt" / "Get charge for a payout" and
before "List payouts") — the dedicated reference page itself could
not be fetched this session (same fetch-tool restriction that blocked
a-1-iv's Create Beneficiary page), so the exact path is inferred from
JuicyWay's own consistent sibling pattern (`GET /bulk-transfers/{id}`
confirmed for "Get bulk payout details") rather than confirmed
directly — flagged the same way a-1-iv's endpoint was, NOT presented
as settled.

**The real finding here, and the reason this isn't a straight
`Korapay.verifyPayout` port:** a-1-i-zo's own worked-example response
(`initiate-bank-transfer.md`) has no `reference` field in its response
body at all — only JuicyWay's own `id`. `Korapay.verifyPayout(reference)`
looks a payout up by the caller's own reference string; nothing
confirms JuicyWay supports that. So `Juicyway.verifyPayout()` here
takes the JuicyWay-assigned `id` (returned from `processPayout()`'s
response as `result.data.id`), not the caller's own reference — this
is the exact "reference-lookup gap" this leaf's own original text
predicted checking for, now confirmed to exist on the payout side too.
**Callers must store `data.id` from the `processPayout()` response if
they want to verify it later** — this codebase does not do that
anywhere yet (a `routes.js`/persistence concern for (e), not this
leaf).

#### a-3. `getBanks` — international bank/institution list lookup [x]

Resolved this session (2026-09-08), fully confirmed against a primary
source (unlike a-1-iv/a-2, no unconfirmed-path caveat needed here):
`docs.juicyway.com/transfers/transfers/list-ngn-banks` documents
**`GET /payment-methods/banks`** directly, including a full worked
response example: `{ "data": [ { "code": "000014", "name": "ACCESS
BANK" }, ... ] }`. Base host `api.spendjuice.com` — the same host
Task 45a already flagged from the bulk-transfers/resolve-account-number
pages, now independently confirmed a third time.

**Real shape difference from Korapay, not a straight port:**
`Korapay.getBanks(currency = 'NGN')` takes a `currency` query param
and is written to support multiple currencies/countries. JuicyWay's
endpoint is explicitly, only "List **Nigerian** Banks" — nothing in
its docs (or anywhere else surfaced this session) suggests an
equivalent multi-currency/multi-country bank-list endpoint exists for
JuicyWay at all. `Juicyway.getBanks()` here takes no currency
argument and does not pretend to support one — if a non-NGN caller
needs this, that's a real product gap to flag to the product owner
(JuicyWay may simply not expose a bank list outside Nigeria), not
something to paper over with an unused parameter.

**Cross-reference, not this leaf's job to fix:** the page's own code
samples show JuicyWay's `Authorization` header written three
different ways across three different doc pages now seen this session
— `Bearer YOUR_API_KEY` (bulk-transfers), plain `YOUR_API_KEY` with no
`Bearer` (resolve-ngn-account-number), and `' YOUR_API_KEY'` with a
stray leading space in the string literal (this page) — all on
`docs.juicyway.com` itself, not a transcription issue on this file's
side. This implementation keeps the existing `Bearer` convention
already used elsewhere in `juicyway.js` for consistency within this
codebase; if that turns out to be wrong for a specific endpoint,
that's a sandbox-testing finding for whoever runs (e)'s wiring, not
something guessable from three contradictory doc samples.

**Cross-reference, not a duplicate:** Juicyway's amount-unit rule for
*collection* (base vs. subunits) is already tracked as its own open
item under Task 49/a — do not re-open that question here; a-1/a-2/a-3
above are about payout-side endpoints existing at all, a distinct gap
from the collection-side amount-unit question.

### b. Korapay — confirm or extend payout/verify-payout/bank-list beyond African rails [ ]

Korapay's `processPayout`/`verifyPayout`/`getBanks` already exist and
work for African rails (Task 42's own history confirms `processPayout`
was fixed and verified). Task 51/b-1 lists Korapay as international
rails' Fallback 1 for payout/verify-payout/bank-list, but that's
currently unconfirmed — Korapay's own currency list
(`CONFIRMED_PROVIDER_CURRENCIES.korapay`) is African-currency-focused
(NGN/GHS/KES/ZAR/USD/XAF/XOF/EGP/TZS), so before this can honestly be
called a working international fallback, confirm whether Korapay's
payout API even accepts a non-African-rail destination (a different
country's bank account, a card, a wallet) at all — this may turn out
to be a hard "no," not just an extension task, in which case Task
51/b-1's Fallback 1 entry needs correcting to say so rather than
implying it just needs more code.

### c. Paystack — build real (non-stub) payout/verify-payout/bank-list [ ]

Paystack's payout/verify-payout/bank-list are stubs today (Task 43's
"stub until fully integrated" state, confirmed still true as of this
session). Needs implementing against Paystack's own transfer/recipient
API (paystack.com/docs — a primary source this file hasn't audited yet
for the transfer surface specifically, only the charge surface per
Task 8) so Paystack can be a genuine fallback in both the African-rails
domain (per Task 51/b-2) and, per Task 51/b-1, a currency-limited
fallback in the international domain too.

### d. Flutterwave — full provider build, from zero [ ]

No `providers/flutterwave.js` exists. This file's own prior research
(search "Flutterwave — FULL API discovery pass" above) already did the
doc-audit; this branch is about turning that research into working
code, which is a different kind of work and should stay its own leaf.

#### d-1. Resolve the v3-vs-v4 version decision [ ]

Blocks every other Flutterwave leaf. This file's research found
Flutterwave "currently publishes two overlapping API generations" with
the older v3 matching this repo's existing provider shape and v4 being
actively promoted as the new default — this is a product decision
(which version to build against), not something resolvable from docs
alone. Needs the product owner's direct input, same pattern as Task 49
resolved JuicyWay's stablecoin question.

#### d-2. Implement `providers/flutterwave.js` against whichever version d-1 picks [ ]

`processPayment`, `verifyTransaction`, `processPayout`, `verifyPayout`,
`getBanks`, `verifyWebhookSignature` — full parity with Korapay's
method set, since Task 51 lists Flutterwave as a fallback candidate in
*both* domains. This file's own Flutterwave research above already has
confirmed findings for several of these (webhook header name,
OAuth/bearer-token flow for v4, base-URL env-select bug to avoid
copying) — reread that section before starting, don't re-research from
scratch.

### e. Routing-layer rewrite — make `routes.js` actually use the Task 51 model [ ]

Only makes sense to pick up once enough of (a)-(d) exist that there's
something real to route to — attempting this first would just be
rewiring `ROUTING_RULES` to point at methods that still throw/501.

#### e-1. Domain-detection logic — decide international vs. African per request [ ]

Nothing today inspects an incoming `/pay`, `/payout`, `/verify`, or
`/banks` request and classifies it as "international" or "African
rails" — Task 51's tables assume that classification already happened.
Needs a real rule (by currency? by destination country? by an explicit
client-supplied field?) — this is itself a product decision, not
purely an engineering one, and should be confirmed with the product
owner before implementing rather than guessed.

#### e-2. Rewrite `ROUTING_RULES`/`getProvider()` to route by domain, with explicit fallback/promote-to-default support [ ]

Once e-1 exists: replace the current flat `action -> provider` map
with domain-aware routing that picks the Task 51 default, and exposes
a way to explicitly request a fallback or (per the product owner's own
stated reason for keeping fallbacks fully built) promote a fallback to
default without a code deploy — whether that's an env var, a config
file, or an admin-dashboard toggle (Task 46 already scoped an admin
dashboard) is an open design question for this leaf, not decided here.

---

## Task 53 — Card issuance assigned to Korapay; white-label branding (name + theme) must be dynamic; KYC/KYB done via PaymentPoint but persisted in this platform's own database [ ]

**Scope note, read first:** decision-record only, per this file's own
established pattern (same discipline as Tasks 0/45/46/47/48/51/52). No
code, no schema, no provider file written this session.

### a. Card issuance — Korapay, white-label

**Stated directly by the product owner this session:** when this
platform's customers (businesses or individuals — "be it a business or
anything") get a card, the underlying issuer is **Korapay**, but the
customer only ever sees this platform's own brand on it. This is the
same identity-concealment posture Task 47 already established for
every other rail this platform offers — card issuance is not a special
case, it just resolves Task 48/e's "planned, provider TBD" line to a
specific answer: **Korapay**.

**Real open item, not resolved by this decision:** Korapay's own Card
Issuing surface (virtual card creation/funding/withdrawal/management)
is real and documented, but per this file's own prior Korapay audit
(search "Not covered by this pass" in the Korapay research section
above), it was **explicitly excluded** from every audit pass done so
far — it's marked **beta** in Korapay's own docs, and this file has
zero confirmed endpoint/auth/payload detail for it. Per Task 0's own
Discovery Convention (every provider surface gets one real docs audit
before any code is written — no assuming a new surface behaves like an
already-integrated one), **Korapay's Card Issuing API needs its own
dedicated discovery pass** before `providers/korapay.js` gains any
card-issuance methods. Do not assume it shares auth/payload shape with
Korapay's existing collection/payout endpoints just because it's the
same provider.

### b. White-label branding — must be dynamic, not hardcoded

**Stated directly by the product owner this session:** the brand name
shown to the customer (today assumed to be "BPay" informally, per this
repo's own name) and the UI theme must both be **configurable at
runtime** — the product owner's own stated reason is the ability to
change either "any time I don't like the name" or the look, without a
code deploy. This is a **new, cross-cutting requirement** on top of
Task 47's identity-concealment decision (Task 47 established *that*
the underlying provider is hidden; this task establishes that the
platform's own presented identity is itself a mutable setting, not a
fixed brand).

**Real open item, not resolved by this decision:** where this
configuration lives and how it's edited is undesigned. Task 46 already
reversed this repo's original no-database constraint specifically to
back an admin dashboard — branding config (name string, theme
values/tokens, presumably a logo asset) is a natural fit for that same
database and that same admin route, but no table, no field list, and
no admin-UI surface for editing it has been designed yet. Flagging
this explicitly so a future session doesn't invent a shape for it
without checking whether Task 46's dashboard schema work has already
settled on one.

### c. Domain-based routing — reaffirmed, not changed

**Stated directly by the product owner this session, restating (not
altering) Task 51's existing decision:** cross-border/international
payments default to **Juicyway**; African rails default to
**Korapay**. This task changes nothing about Task 51's tables or
Task 52's implementation breakdown — it's recorded here only because
the product owner restated it in the same message as (a)/(b)/(d), and
this file's own convention is to record what the product owner says
even when it confirms an existing decision, so a future session
doesn't wonder whether something changed. **See Task 51 (b-1/b-2) and
Task 52 for the actual tables and implementation gaps — this
sub-section is a pointer, not a duplicate.**

### d. KYC/KYB — PaymentPoint as the verification provider, but this platform owns the record

**Stated directly by the product owner this session:** identity
verification (KYC for individuals, KYB for businesses) is done through
**PaymentPoint** specifically — the product owner's own stated reason
is that PaymentPoint has the relevant **licenses and compliance
posture** ("because they have licenses check and other things"), not a
technical preference. PaymentPoint's own NIN/BVN verification and
liveness-check endpoints were already fully audited under Task 0/a-8
(see "PaymentPoint — FULL API discovery pass" in the Confirmed
research findings section) — this task assigns that already-audited
capability to the KYC/KYB role, it doesn't newly audit anything.

**The one part of this that IS new, and matters architecturally: the
verification result must be persisted in this platform's own
database, not left to live inside PaymentPoint alone.** Product
owner's own stated reason: so a customer verified once can be reused
"in any other payment provider like the Kora and Juicyway that are my
default" — i.e. this platform, not PaymentPoint, is the source of
truth for "is this customer/business verified," and Korapay/Juicyway/
any future provider consult *this platform's* record rather than each
independently re-running (or being asked to trust) a PaymentPoint
verification.

**Explicit and deliberate: Korapay has its own competing Identity/
KYC & KYB feature** (NG/ZA/GH/KE/US/CI coverage, BVN/NIN/vNIN/SSN/
passport/national-ID/phone verification, liveness check — see
Korapay's own audit section above, also explicitly excluded from
Korapay's prior audit passes same as Card Issuing above). **This
platform is deliberately NOT using Korapay's own KYC/KYB feature for
this role, in favor of PaymentPoint** — flagging this explicitly so a
future session doesn't "helpfully" default to Korapay's built-in
identity feature just because Korapay is already an integrated,
already-audited-for-other-purposes provider. That would be exactly
the wrong assumption here.

**Real open items, not resolved by this decision:**
- No schema exists yet for the persisted verification record. At
  minimum this needs to capture: which entity was verified (customer
  vs. business), verification status, the underlying PaymentPoint
  reference/response (for audit trail), and a way for Korapay/
  Juicyway-facing code to query "is this entity verified" without
  re-fetching from PaymentPoint. Field-level design is genuinely open
  — not decided here, same caution Task 45/b already applied to the
  Reseller/VTU Supabase schema proposal (that proposal is a *different*
  set of tables, for a *different* product — don't conflate the two).
- Row-Level Security / access-control design for who (this platform's
  own backend only? a future business-facing dashboard?) can read a
  stored verification result is undesigned, same open question Task
  45/b already flagged for its own schema proposal.
- PaymentPoint's own audit (Task 0/a-8) already flagged a live-looking
  Bearer token + api-key pair exposed directly in PaymentPoint's own
  docs pages — unrelated to this task's scope, but worth re-flagging
  here since this task makes PaymentPoint's role load-bearing
  (compliance-critical, not just a nice-to-have integration) in a way
  it wasn't before this decision.

### e. Not yet done, this session, deliberately

No code, no schema, no `providers/*.js` changes. This task exists so
the next session has all three decisions on record in one place before
picking up implementation — in particular, Task 52's `X` marker
(Juicyway `processPayout`, Task 52/a-1) is unaffected by this task and
remains the current single atomic unit of work on the board; card
issuance, dynamic branding, and KYC/KYB persistence are all real,
scoped-but-not-started work that a future session should turn into
their own numbered leaves (following this file's own Task Numbering &
Workflow Convention) once Task 52's board is far enough along that
they're not competing for the single `X` slot.

---

## Task 54 — KYC/KYB reframed as default+fallback (PaymentPoint default, swappable); new Gift Cards & VTU domain routing decided (telcos.opik.net default, Flutterwave VTU fallback); Prestmit demoted from standalone provider list [ ]

**Scope note, read first:** decision-record only, same discipline as
Tasks 51/52/53. No code, no schema, no `providers/*.js` changes this
session. Per the Patch Handoff Convention, a `git format-patch` file
covering this handover.md update is generated and handed to the
product owner directly this session — not applied or pushed here.

### a. KYC/KYB — now a default+fallback table, not a single fixed provider

**Stated directly by the product owner this session, refining Task
53/d:** PaymentPoint is the **default** identity-verification provider
(individual KYC, business KYB), not the only one — same
default-with-swappable-fallback shape Task 51 already established for
payment routing. Any provider that has its own competing KYC/KYB
capability (Korapay's own Identity/KYC-KYB feature, per Task 53/d's
own note that this platform was "deliberately NOT using" it — that
posture is now a *default choice*, not an exclusion) is a fallback
that **can be promoted to default at any time**, same reasoning as
every other domain in this file: a fallback can't be allowed to lag
behind the default in capability just because it isn't active today.

| Field | Value |
|---|---|
| Domain covers | Individual KYC, business KYB, for every rail/domain in this file (payments, gift cards, VTU, card issuance) — one verification result, reused everywhere, per Task 53/d |
| **Default** | **PaymentPoint** — chosen for licensing/compliance posture (Task 53/d), not technical preference; NIN/BVN verification + liveness check, fully audited (Task 0/a-8) |
| Fallback 1 | Korapay's own Identity/KYC & KYB feature (NG/ZA/GH/KE/US/CI, BVN/NIN/vNIN/SSN/passport/national-ID/phone, liveness check) — real and documented, but **explicitly excluded from every Korapay audit pass to date** (same exclusion as Korapay Card Issuing, Task 53/a) — needs its own dedicated discovery pass before it can genuinely serve as a fallback, not just be listed as one |
| Fallback 2+ | Any other provider's own KYC/KYB surface, if/when audited — none currently confirmed beyond Korapay's |
| Persistence | Unchanged from Task 53/d: whichever provider performs the check, the result is written to this platform's own DB (Supabase) as the source of truth, so a customer verified once is not re-verified per rail or per provider |
| Changelog | 2026-09-07 (later same day) — table created this session, per product-owner direction (this task); reframes Task 53/d's "PaymentPoint, deliberately not Korapay" language as "PaymentPoint default, Korapay (or others) swappable fallback" — the underlying persistence-in-Supabase architecture from Task 53/d is unchanged |

**Real open item, not resolved by this decision:** same schema/RLS
gaps Task 53/d already flagged — no table exists yet, and the promote-
to-default mechanism (env var / config / admin toggle) is the same
open design question Task 52/e-2 already raised for payment routing;
this task does not re-decide it, just notes KYC/KYB now needs the same
mechanism once it's built.

### b. New domain: Gift Cards & VTU (airtime/data/eSIM, local and international)

**Stated directly by the product owner this session:** two more
domains get the same default+fallback routing treatment as Task 51's
international/African rails — **Gift Cards**, and **VTU** (airtime,
data, eSIM — both local and international). Both default to
**`telcos.opik.net`**, the product owner's own personal reseller
endpoint (Task 45), for the same reason already on record there: it
aggregates the underlying providers for these rails internally, so
this platform integrates one contract instead of many. The product
owner's own stated reason gift cards specifically default there:
`telcos.opik.net` already has **two** underlying gift-card providers
behind it — **Prestmit and Tremendous** — giving it built-in
redundancy at the aggregator level before this platform's own
fallback layer is even considered.

| Field | Value |
|---|---|
| Domain covers | Gift cards (buy/sell/liquidation, per Task 48/a's existing "Gift Cards" public framing); airtime, data, and eSIM reselling, local and international |
| **Default** | **`telcos.opik.net`** — for both gift cards and VTU; internally backed by Prestmit + Tremendous for gift cards specifically (per product owner), plus whatever underlying VTU providers it aggregates (Lizzysub/Zendit/Accragh per Task 48/b, staying internal to `telcos.opik.net` same as before) |
| Fallback 1 (VTU only) | **Flutterwave** — stated by the product owner as the fallback for VTU services specifically. **Not yet implemented** (`providers/flutterwave.js` does not exist; still blocked on the v3-vs-v4 decision, Task 48/e) — must be built before it can actually serve as a fallback, same caveat as every other Flutterwave fallback listing in this file (Task 51/b-1, b-2) |
| Fallback (gift cards) | **Not stated this session — open item.** The product owner named a VTU fallback (Flutterwave) but not a gift-card fallback; not assumed here. Flag for a future session to confirm rather than defaulting to Flutterwave for gift cards too |
| Changelog | 2026-09-07 (later same day) — table created this session, per product-owner direction (this task); first time Gift Cards and VTU are given their own routing table, previously only covered narratively (Task 44, 45, 48) |

### c. Prestmit demoted from standalone provider list

**Stated directly by the product owner this session:** Prestmit is
**not** one of this repo's standalone providers going forward — it is
one of the underlying providers `telcos.opik.net` aggregates for gift
cards (alongside Tremendous, per (b) above), consistent with Task
47/48's identity-concealment posture (Prestmit was already never named
to end users) and with `telcos.opik.net`'s own existing treatment of
its other internal VTU providers (Lizzysub/Zendit/Accragh, Task 48/b).

This **does not delete** Task 0/a-9's Prestmit discovery audit or Task
48/a's scope-confirmation record — per this file's own history
discipline (preserve what was found and decided, don't overwrite), 
those entries stay as the record of what was researched and when.
**What changes going forward:** any future task, routing table, or
provider count in this file should treat Prestmit as internal to
`telcos.opik.net`'s gift-card capability, not as a candidate for its
own `providers/prestmit.js`. No code exists for Prestmit today (`ls
providers/` confirms only `juicyway.js`/`korapay.js`/`paystack.js`
exist), so this is a documentation-scope correction only, not a code
removal.

**Real open item, not resolved by this decision:** if `telcos.opik.net`
itself is ever audited (Task 0/a-10, still not started), that audit
should confirm whether Prestmit/Tremendous are reachable as distinct
gift-card sub-choices through `telcos.opik.net`'s own API (e.g. a
provider-select field) or whether `telcos.opik.net` picks between them
internally with no visibility to this platform — undecided until that
audit happens.

### d. Not yet done, this session, deliberately

No code, no schema, no `providers/*.js` changes, no `ROUTING_RULES`
changes. This task exists so the next session has all three decisions
(KYC/KYB fallback model, Gift Cards & VTU domain routing, Prestmit's
demotion) on record before picking up implementation. Task 52's `X`
marker (Juicyway `processPayout`, Task 52/a-1) is unaffected and
remains the current single atomic unit of work on the board.

---
