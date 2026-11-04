# Release Provenance: Signed, Verifiable `tmite` Releases

Created: 2026-11-04
Author: me@tane.dev
Agent: Claude Code
Status: DRAFT
Approved: No
Worktree: No
Type: Build

## Summary

**Goal:** Anyone, or any system, can check that the `tmite` they run is a
real release: built from this repository, by its release pipeline, from a
tagged commit, and not changed since. And `tmite` can check itself.

**Why:** termite guards a terminal, so a tampered `tmite` is worth a lot to
an attacker. The easiest attack is a build with the guards switched off. A
binary cannot hold its own hash, so the proof has to live outside it, in
public, signed records.

**Today:** the repository has no GitHub remote yet, and there is no
pipeline. `gh` 2.103 (with `gh attestation`) is installed on this machine.

## How it works

1. **Builds happen only in GitHub Actions**, on a protected `v*` tag, on
   GitHub-hosted runners: macOS arm64/x86_64 and Linux arm64/x86_64.
2. **Every artifact gets a build provenance attestation**
   (`actions/attest-build-provenance`). This is Sigstore keyless signing:
   the signature is tied to this repository, the release workflow file and
   the tag, and it is recorded in the public Rekor transparency log. No
   signing key is stored anywhere to be stolen.
3. **A release record**, `releases.json`, is published on GitHub Pages
   (later on the owner's own domain). It holds, for each version: the
   version, date, commit, the SHA-256 of each artifact, and the hash of the
   previous record. The chain means history cannot be rewritten without it
   showing. The record is attested in the same way.
4. **Distribution:**
   - a Homebrew tap, whose formula pins each artifact's SHA-256;
   - `cargo install --locked`, for people who build it themselves;
   - GitHub Releases, carrying the attestation bundles.
5. **Verifying:**
   - **By hand:** `gh attestation verify tmite --repo <owner>/termite`, or
     compare `shasum -a 256 tmite` with `releases.json`.
   - **`tmite --version --verify`:** hash the running executable, fetch
     `releases.json` and its attestation, check the attestation against
     the identity built into `tmite` (repository, workflow path, tag
     pattern), then compare. The binary holds the expected *identity*, not
     its own hash, so there is no chicken-and-egg problem.
   - **Rollback check:** termite's provenance log records the newest
     version it has seen verified. A verified but older release is
     reported as a downgrade.
   - **Offline:** reported as "not verified (offline)", never as verified.

## Threats to the pipeline, and what stops them

| Threat | Mitigation |
|---|---|
| a malicious or compromised dependency | `Cargo.lock` committed, `--locked` builds; `cargo-deny` (advisories, licences, sources: crates.io only); `cargo-audit` in CI; review every dependency added (already a project rule) |
| a compromised third-party Action | as few Actions as possible, each pinned to a full commit SHA, not a tag; Dependabot proposes updates for review |
| a stolen or over-broad token | `permissions: read-all` by default; `id-token: write` and `contents: write` only in the release job; no secrets in pull-request workflows; never `pull_request_target` |
| a forged release tag | a tag ruleset (only maintainers, no force-push, no deletion), signed tags, a required review on `main` |
| a tampered runner or a build reaching out | GitHub-hosted, ephemeral runners only; egress limited to crates.io and GitHub during the build (for example with `step-security/harden-runner`) |
| cache poisoning | no caches in the release job |
| swapped artifacts after the build | attestations bind the artifact's digest; the Homebrew formula pins the SHA-256; `releases.json` is attested and chained |
| a freeze or rollback (serving an old, vulnerable version) | the chained `releases.json`; the rollback check in `tmite` |
| a one-off build that cannot be reproduced | a second, independent build job compares artifact hashes, and the release fails if they differ |
| a fake `tmite` on PATH | `tmite --version --verify` and the Homebrew checksum; `tmite setup` tells you which binary is first on PATH |
| macOS Gatekeeper | Developer ID signing and notarization for the macOS binaries (Owner Question 2) |

## Acceptance Criteria

- [ ] AC1: A tag push builds all four targets, attests each one, publishes
      a GitHub Release and updates the chained, attested `releases.json`.
- [ ] AC2: `gh attestation verify` passes for each artifact and fails for a
      changed one.
- [ ] AC3: `tmite --version --verify` reports verified, changed, unknown,
      downgrade and offline correctly (tests against a fixture record and
      bundle).
- [ ] AC4: Every Action is pinned by SHA; `cargo-deny` and `cargo-audit`
      pass; workflow permissions are read-only except in the release job
      (checked by a workflow lint, for example `zizmor`).
- [ ] AC5: Two independent builds of the same tag give identical hashes.

## Owner Questions

1. **GitHub home:** which owner or organisation, and the repository name
   (`termite`)? A remote is needed before anything here can start.
2. **macOS signing:** is there an Apple Developer ID for code signing and
   notarization? Without one, macOS binaries still get Sigstore
   attestations, but Gatekeeper warns.
3. **How `tmite` verifies:**
   - **(a)** built in, with the `sigstore` crate: self-contained, but a
     large new dependency tree in the binary we are trying to keep small
     and auditable;
   - **(b)** call `gh attestation verify` or `cosign` when installed, and
     otherwise check only the SHA-256 against `releases.json` (which is
     itself served over HTTPS from GitHub Pages).

   Proposed: (b) first.
4. **Record location:** GitHub Pages now, the owner's domain later
   (proposed). Or both from the start?

## Round Log

- Draft (2026-11-04): from the owner's request for signed releases,
  pipeline provenance and a public record that `tmite` can check itself
  against.
