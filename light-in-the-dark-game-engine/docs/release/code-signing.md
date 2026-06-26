# Code Signing — Investigation & Findings (#183)

Status: **investigation complete; implementation gated** on the live download page +
release artifact pipeline (#182 produces the artifacts + checksums; the live page
is hosting-gated) and on purchasing certificates / provisioning key custody.

Own-site distribution (D-2026-06-11-22) means the OS trust prompt *is* the
first-run experience: an unsigned Windows binary hits SmartScreen, an unsigned/
un-notarized macOS app hits Gatekeeper. Signing removes those walls. No
third-party SDKs in the binaries (D-2026-06-11-21); signing is a build/release
step, not a runtime dependency.

This doc records the chosen mechanism per OS, cost/renewal/custody, and the
pipeline-enforcement design. Dollar figures are ranges to confirm at purchase —
vendor pricing drifts; the decision rationale is what is durable here.

## Per-OS mechanism (decision + rationale)

### Windows — Authenticode

- **Mechanism:** sign the `.exe` (and installer, if any) with `signtool sign /fd
  sha256 /tr <RFC3161-timestamp-URL> /td sha256`. A timestamp countersignature is
  mandatory — without it, signatures expire when the cert does; with it, already-
  signed binaries stay valid past cert expiry.
- **OV vs EV — decision:** **start OV**, revisit EV only if SmartScreen reputation
  ramp proves too slow.
  - OV (Organization Validation): ~$200–400/yr. Signs validly, but SmartScreen
    reputation accrues *per publisher* over download volume — early users may
    still see a "not commonly downloaded" warning until reputation builds.
  - EV (Extended Validation): ~$300–700/yr, hardware-token or cloud-HSM key
    custody required, heavier vetting. Grants immediate SmartScreen reputation (no
    ramp). Worth it only if the OV reputation ramp is a real adoption blocker.
- **Verify (FSV):** `signtool verify /pa /v <artifact.exe>` — must report a valid
  chain + timestamp.

### macOS — Developer ID + notarization + stapling

- **Mechanism (three steps, all required):**
  1. `codesign --force --options runtime --timestamp --sign "Developer ID
     Application: <Team>" <app>` — hardened runtime is required for notarization.
  2. `xcrun notarytool submit <zip|dmg> --wait` — Apple scans + returns a ticket.
  3. `xcrun stapler staple <app>` — attach the ticket so first-run works offline.
- **Account:** Apple Developer Program, **~$99/yr**. Developer ID cert issued from
  the account; renewal tracks membership.
- **Verify (FSV):** `spctl -a -vv <app>` → "accepted, source=Notarized Developer
  ID"; `stapler validate <app>` → "The validate action worked".

### Linux — detached signature + sums

- **Mechanism:** **minisign** (Ed25519, single small public key, no web-of-trust
  overhead) over each artifact, alongside the SHA-256 sums the release tool
  (`tools/release`) already emits. Decision: minisign over GPG — one short public
  key to publish, simpler verify instructions for users, no keyring management.
  GPG remains an acceptable alternative if a distro packaging path later needs it.
- **Cost:** none (self-managed keypair).
- **Verify (FSV):** `minisign -Vm <artifact> -P <public-key>` → "Signature and
  comment signature verified".

## Key custody (D: keys never in repo)

- Signing keys live **only** in the release operator's secret store — never
  committed, never in build logs. The repo carries **public** material only: the
  minisign public key (for users to verify Linux artifacts) and the documented
  cert thumbprints.
- Windows EV (if adopted) and macOS keys are hardware-token / Keychain resident on
  the operator's signing machine; the release pipeline calls the local signing
  tools, it does not transport private keys.
- **Custody record to maintain** (off-repo): who holds each key, where, renewal
  date, and the revocation procedure if a key is compromised.

## Pipeline enforcement (D: unsigned artifacts cannot reach the page)

The release flow gains a **verify-before-publish gate**: after signing, the
pipeline re-verifies every artifact with the per-OS verifier above and refuses to
upload to the download page if any artifact is unsigned or fails verification.
This composes with the existing `tools/release` integrity manifest (SHA-256 per
artifact, consumed by `litd/updatecheck`): integrity (tamper detection) and
authenticity (signature) are separate gates, both required.

Enforcement shape (mirrors the no-CI local gate model): a release script step that
runs `signtool verify` / `spctl`+`stapler` / `minisign -V` on the staged
artifacts and exits nonzero on any failure, before the publish step runs.

## FSV plan (when implemented)

Source of truth = the verifier output on **real downloaded** artifacts, not exit
codes alone.

- Download each artifact from the live page; run the three verifiers above and
  read all outputs; first-run each binary on a clean VM and record the trust
  prompt (or its absence).
- Edge 1 — push a deliberately **unsigned** build through the pipeline → blocked
  before publish (capture the gate failure).
- Edge 2 — flip a byte in a signed binary → verification fails loudly (capture
  `signtool`/`spctl`/`minisign` rejection).
- Edge 3 — notarization rejection (a hardened-runtime violation) → pipeline
  surfaces Apple's log excerpt.

## Implementation gating (what's left on #183)

1. **Purchase + custody:** OV cert (Windows), Apple Developer membership (macOS),
   generate + publish the minisign keypair (Linux). Blocked on operator
   spend/decision.
2. **Pipeline signing steps** + the verify-before-publish gate — blocked on (1)
   and on the live download page existing (the issue's stated blocker).
3. **Clean-machine first-run capture** for the FSV evidence — needs the signed
   artifacts + clean VMs.

The investigation half (mechanism, cost, custody, enforcement design) is recorded
here; the signing/notarization implementation and its on-real-artifact FSV remain
gated as above.
