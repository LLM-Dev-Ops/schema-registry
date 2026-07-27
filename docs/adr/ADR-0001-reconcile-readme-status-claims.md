# ADR-0001: Reconcile README Status Claims

**Status:** Proposed
**Date:** 2026-07-27
**Deciders:** Schema Registry Maintainers
**Implements:** [ADR-013: README Claim Substantiation](../../../agentics-enforcement/plans/adr/ADR-013-readme-claim-substantiation.md) (agentics-enforcement)

---

## Context

This repo's README asserts production readiness while contradicting itself on every
quantitative claim it makes. All citations read from the working tree on 2026-07-27.

**Badges** (`README.md` lines 5-9) are all hardcoded `img.shields.io/badge/` literals:

- Line 7: `[![Status](https://img.shields.io/badge/status-production_ready-brightgreen.svg)](./plans/)`
- Line 9: `[![Build](https://img.shields.io/badge/build-passing-brightgreen.svg)](./.github/workflows)`

**The status block contradicts itself and the rest of the file:**

| Claim | Location | Conflicting claim | Location |
|---|---|---|---|
| `550+ tests planned` | line 20 | `✅ 550+ tests implemented (Unit, Integration, E2E, Property)` | line 254 |
| `8 tests passing` | line 17 | `schema-registry-core ... (15 tests passing)` — one crate | line 265 |
| `Cargo workspace with 10 crates` | line 17 | `all 13 crates` | line 251 |

`Cargo.toml` lines 2-19 declare **17** workspace members, against **19**
directories in `crates/`. So the crate count appears in three mutually exclusive
forms, none matching the manifest.

**Measured ground truth:**

- 656 `#[test]` / `#[tokio::test]` attributes across 56,574 lines of Rust in `crates/`.
- **No `Cargo.lock` exists anywhere in the repo.** Nothing here has a reproducible
  build, which makes the line 9 `build-passing` badge unverifiable by construction
  and the line 17 "zero compilation errors" claim unsupported.

The README also links `plans/SPARC-COMPLETION-CERTIFICATE.md` from line 29 as the
"🏆 **Final deliverables summary**" addressed to "**Executives, stakeholders**" —
a completion certificate for a workspace that has never resolved its dependencies.

Note the direction of the error: 656 test attributes exist in-tree, but the README
advertises 8. The problem is not that the repo is emptier than claimed — it is that
**no figure in the README came from an executed run**, so the figures drifted apart
from each other and from the manifest in both directions.

## Decision

**Every quantitative claim in this README is either regenerated from a real run or
deleted. No claim survives on the strength of having been typed.**

Per ADR-013 Rule 1, the `build-passing` badge (line 9) is replaced with a native
GitHub Actions workflow badge. Per Rule 4, the `production_ready` status badge
(line 7) is withheld until a committed `Cargo.lock` and a green build exist.

Concretely:

1. Commit a `Cargo.lock` — the precondition for every other claim here.
2. Replace line 9's badge with
   `https://github.com/globalbusinessadvisors/llm-schema-registry/actions/workflows/ci.yml/badge.svg`.
3. Reword line 7 from `status-production_ready` to `status-alpha-orange` until the
   build is green and the test count is CI-derived.
4. Replace lines 17 and 20 with a single CI-generated figure. Delete the
   `550+ tests planned` / `550+ tests implemented` pair outright — "planned" is not
   a test result, and "implemented" was never measured.
5. Correct the crate count to the manifest value (17 members) in one place, and
   remove the restatements at lines 17 and 251.
6. Retitle `plans/SPARC-COMPLETION-CERTIFICATE.md` to a design-completion record,
   or move it under `plans/history/`. A methodology phase completing is not the
   product being complete, and the current framing invites executives to read it
   as the latter.

## Consequences

### Positive

- One test count, derived from `cargo test`, replaces four mutually contradictory ones.
- A committed `Cargo.lock` makes the build reproducible, which is a prerequisite
  for any honest status claim and for CI to run at all.
- Downgrading to `alpha` is recoverable; a `production_ready` badge on an
  unbuildable workspace is a credibility loss that is not.

### Negative

- The README will advertise a much smaller verified surface than it does today.
  This is the correct outcome: the current numbers are not evidence.
- Committing `Cargo.lock` may surface genuine resolution failures across 17
  members. Better discovered by maintainers now than by an adopter.

### Risks

- Resolution may fail against the same unpublished LLM-Dev-Ops crates that block
  `auto-optimizer` (see that repo's ADR-0001). Mitigation: feature-gate or path-pin
  affected members; do not restore the badge until the workspace resolves.

## Implementation Plan

1. Run `cargo generate-lockfile`; commit the resulting `Cargo.lock`.
2. Fix any resolution failures, feature-gating unresolvable optional members.
3. Add a `.github/workflows/ci.yml` job running `cargo build --workspace` and
   `cargo test --workspace`, emitting the real test count.
4. Replace `README.md` line 9 with the workflow badge from that job.
5. Change `README.md` line 7 to `status-alpha-orange`.
6. Rewrite `README.md` lines 15-21: single CI-derived test count; delete the
   "550+ planned" claim; state the manifest crate count (17).
7. Delete `README.md` lines 253-262's unverified testing-infrastructure bullet
   list, or replace each with a figure the CI job emits.
8. Correct `README.md` line 251 (`all 13 crates`) and line 265's per-crate count
   to CI output.
9. Relocate or retitle `plans/SPARC-COMPLETION-CERTIFICATE.md`; update the
   line 29 link.
10. Restore `status-production_ready` only when steps 1-8 are green and coverage is
    published by CI.

## Verification

- [ ] `Cargo.lock` exists at repo root and is tracked by git.
- [ ] `cargo build --workspace --locked` succeeds from a clean checkout.
- [ ] `cargo test --workspace` runs, and the count it reports is the only test
      count appearing anywhere in `README.md`.
- [ ] `grep -n "550+" README.md` returns nothing.
- [ ] The crate count in `README.md` matches `Cargo.toml` members, and appears once.
- [ ] `grep -c "img.shields.io/badge/build" README.md` returns 0.
- [ ] `npm run check:claims-honesty` (agentics-enforcement) exits 0 for this repo.
