# canton/ — Bill of Exchange tokenization on Canton Network

Canton-side counterpart to `crates/boe_instrument`. See HANDOVER.md's
"chain choice resolved as Canton/Daml" section for why this replaced the
originally-spec'd ERC-3643/T-REX (EVM + Solidity) path: T-REX's only
actively-maintained implementation is GPLv3, which doesn't fit this
workspace's Apache-2.0 licensing, and no permissively-licensed full
substitute exists. Daml and Daml Finance (both Apache-2.0) don't have that
problem, and don't need a bolted-on Identity Registry/Compliance contract
the way plain ERC-20 does — Canton's own authorization model (who can be a
signatory/observer on a contract) covers that natively.

## Layout

- `daml/BillOfExchange/Instrument.daml` — the `BillOfExchangeInstrument`
  template: issuer-signed, drawer/drawee/payee as observers, carrying the
  instrument's disclosed `instrumentId` / `cryptoSignalHex` / `contentHash`.
- `daml.yaml` — SDK version pinned to `2.10.0`, matching daml-finance's own
  pin at the time this was written.
- `Dockerfile` — bakes the Daml SDK into the image at build time (per
  request, since this dev sandbox can't reach `get.daml.com` to install it
  live).

## Status — unverified

Nothing here has been run through the actual Daml toolchain. This sandbox
can clone the (Apache-2.0) `daml-finance` repo from GitHub for reference,
but the Daml SDK installer (`get.daml.com`) isn't on this sandbox's network
allowlist, so `daml build` has not been run against `Instrument.daml`, and
the `RUN daml build` step in `Dockerfile` is unverified. Treat the type
shapes here as the intended design, not a confirmed-working build.

## Not yet done

- **Not wired to Daml Finance's interfaces.** `BillOfExchangeInstrument` is
  a standalone template, not yet an implementation of
  `Daml.Finance.Interface.Instrument.Base.V4.Instrument` (the interface its
  own `ZeroCoupon`/`FixedRate` bond instruments implement, which is what
  would let this plug into Daml Finance's Holding/Account/lifecycle
  machinery instead of standing alone). That needs the daml-finance
  interface DARs pinned as `data-dependencies` in `daml.yaml` (commented
  out there for now) and implementing `getKey`/`GetView` plus
  `Disclosure.I` — left as follow-up rather than guessed at blind.
- **No Ledger API integration.** `crates/boe_instrument/src/canton_bridge.rs`
  produces the Rust-side data (`InstrumentDisclosure`, resolved
  `CantonParty` ids) but does not call any Ledger API/JSON API itself —
  whatever service actually submits the `CreateCommand` to Canton using
  that data is separate, not-yet-built integration work.
- **Party onboarding is assumed, not designed.** `canton_bridge::PartyIds`
  takes Canton party ids as given; how drawer/drawee/payee actually get
  onboarded as Canton parties in the first place isn't addressed here.
- **`VerifyDisclosure`'s authorization reasoning is unconfirmed.** It
  restricts `verifier` to drawer/drawee/payee via an in-choice `assertMsg`
  rather than the ledger's own controller-authorization mechanism —
  double-check this is the right pattern (vs. e.g. requiring `verifier` be
  pre-declared) once this actually compiles and runs.
