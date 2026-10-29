# Request-boundary hardening cycle — 2026-10-22

## Frozen baseline and acceptance gate

Baseline commit: \`d0d89f267c32a2d31b622ad112836435c2534f39\`.

Before candidate changes, the repository-native Node suite passed 35/35 assertions.
MetaHarness 0.4.2 scored the repository at harness fit 76, compile confidence 100,
task coverage 65, tool safety 100, and memory usefulness 47. Darwin 0.10.3
verified the repo-native benchmark (\`60a89a245cf2…\`) and ran one bounded
generation with two deterministic children, concurrency one, seed 20261022,
real sandbox, and risk budget zero. The baseline remained the winner
(\`0.985\`; delta \`+0.000\`), so no harness mutation was accepted.

The implementation gate is frozen to eight request-boundary checks:

1. browser origins are denied when no allowlist is configured;
2. an explicitly configured origin is allowed;
3. a non-allowlisted origin remains denied;
4. every response emits \`X-Correlation-ID\`;
5. response header and JSON metadata use the same request identity;
6. correlation IDs longer than 128 characters are replaced;
7. malformed JSON receives the structured JSON error contract; and
8. a POST without a JSON content type cannot escape the structured contract.

Five behavior anchors must also remain green: \`npm test\`, valid schema
validation, structured 404s, health availability, and generated execution IDs.

## Configuration

Set \`CORS_ALLOWED_ORIGINS\` to a comma-delimited list of exact browser origins.
It is deny-by-default. \`*\` is honored only when explicitly configured. This
does not replace IAM authentication; it limits browser cross-origin access.

## Reproduction

Run from an isolated candidate checkout while preserving a read-only checkout
of the baseline commit:

\`\`\`bash
node scripts/evaluate-request-boundary.mjs \
  /absolute/path/to/baseline \
  /absolute/path/to/candidate \
  /home/ruv/.local/share/llm-dev-ops-tools \
  /tmp/schema-registry-request-boundary-flywheel.json
\`\`\`

The evaluator uses Flywheel 0.1.12 for the signed promotion lineage and replay
verification. It executes live HTTP probes and the real Node test command in
both checkouts. It records the parent bytes and SHA-256 needed for rollback.

## Rollback

Revert the resulting commit. No database, migration, deployment, package
publication, or persistent service is introduced.
