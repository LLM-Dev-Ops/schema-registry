import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const [baseline, candidate, runtime, output] = process.argv.slice(2).map(resolve);
assert.ok(baseline && candidate && runtime && output, 'usage: node evaluate-request-boundary.mjs BASELINE CANDIDATE PINNED_TOOLS OUTPUT');

const packageVersion = (name) => JSON.parse(
  readFileSync(join(runtime, 'node_modules', name, 'package.json'), 'utf8'),
).version;
assert.equal(packageVersion('@metaharness/flywheel'), '0.1.12');

const {
  makeSigner,
  meetsPromotionRule,
  runFlywheelGenerations,
  verifyReplayBundle,
} = await import(pathToFileURL(join(
  runtime,
  'node_modules/@metaharness/flywheel/dist/index.js',
)));

const probeSource = String.raw`
const http = require('node:http');
const path = require('node:path');
const repo = process.argv[1];
const app = require(path.join(repo, 'index.js')).handler;
const request = (port, method, route, body, headers = {}) => new Promise((resolve, reject) => {
  const req = http.request({ hostname: '127.0.0.1', port, method, path: route, headers }, (res) => {
    let raw = '';
    res.on('data', (chunk) => { raw += chunk; });
    res.on('end', () => {
      let json = null;
      try { json = JSON.parse(raw); } catch {}
      resolve({ status: res.statusCode, headers: res.headers, raw, json });
    });
  });
  req.on('error', reject);
  if (body !== undefined) req.write(body);
  req.end();
});
const server = app.listen(0, '127.0.0.1', async () => {
  const port = server.address().port;
  try {
    delete process.env.CORS_ALLOWED_ORIGINS;
    const defaultDenied = await request(port, 'GET', '/v1/schema-registry/health', undefined, {
      Origin: 'https://unconfigured.example',
    });

    process.env.CORS_ALLOWED_ORIGINS = 'https://allowed.example';
    const allowed = await request(port, 'GET', '/v1/schema-registry/health', undefined, {
      Origin: 'https://allowed.example',
    });
    const denied = await request(port, 'GET', '/v1/schema-registry/health', undefined, {
      Origin: 'https://denied.example',
    });
    const correlation = await request(port, 'GET', '/v1/schema-registry/health', undefined, {
      'X-Correlation-ID': 'trace-evaluator-123',
    });
    const overlong = 'x'.repeat(129);
    const sanitized = await request(port, 'GET', '/v1/schema-registry/health', undefined, {
      'X-Correlation-ID': overlong,
    });
    const malformed = await request(port, 'POST', '/v1/schema-registry/validate', '{', {
      'Content-Type': 'application/json',
      'Content-Length': '1',
    });
    const noContentType = await request(
      port,
      'POST',
      '/v1/schema-registry/validate',
      JSON.stringify({ schema_type: 'json_schema' }),
      { 'Content-Length': '29' },
    );
    const valid = await request(
      port,
      'POST',
      '/v1/schema-registry/validate',
      JSON.stringify({ schema_content: '{"type":"object"}', schema_type: 'json_schema' }),
      { 'Content-Type': 'application/json' },
    );
    const unknown = await request(port, 'GET', '/v1/schema-registry/missing');
    const holdout = [
      defaultDenied.headers['access-control-allow-origin'] === undefined,
      allowed.headers['access-control-allow-origin'] === 'https://allowed.example',
      denied.headers['access-control-allow-origin'] === undefined,
      correlation.headers['x-correlation-id'] === 'trace-evaluator-123',
      correlation.json?.execution_metadata?.trace_id === 'trace-evaluator-123',
      sanitized.headers['x-correlation-id'] !== overlong
        && sanitized.headers['x-correlation-id']?.length <= 128,
      malformed.status === 400
        && malformed.headers['content-type']?.includes('application/json')
        && malformed.json?.error === 'Request body contains invalid JSON',
      noContentType.status === 400
        && noContentType.headers['content-type']?.includes('application/json')
        && noContentType.json?.execution_metadata,
    ];
    const anchor = [
      valid.status === 200 && valid.json?.valid === true,
      unknown.status === 404 && unknown.json?.execution_metadata,
      correlation.status === 200 && correlation.json?.status === 'healthy',
      typeof correlation.json?.execution_metadata?.execution_id === 'string',
    ];
    process.stdout.write(JSON.stringify({ holdout, anchor, observations: {
      defaultDenied, allowed, denied, correlation, sanitized, malformed, noContentType, valid, unknown,
    } }));
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  } finally {
    server.close();
  }
});
`;

const measurements = [];
const runProbe = (dir) => {
  const child = spawnSync(process.execPath, ['-e', probeSource, dir], {
    cwd: dir,
    encoding: 'utf8',
    timeout: 120000,
    maxBuffer: 8 * 1024 * 1024,
    env: { ...process.env },
  });
  assert.equal(child.signal, null, 'probe timed out');
  assert.equal(child.status, 0, child.stderr || child.stdout);
  return { report: JSON.parse(child.stdout), command: {
    argv: [process.execPath, '-e', '<request-boundary-probe>', dir],
    exit: child.status,
    stderr: child.stderr.slice(-3000),
  } };
};

const runNative = (dir) => {
  const child = spawnSync('npm', ['test'], {
    cwd: dir,
    encoding: 'utf8',
    timeout: 120000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert.equal(child.signal, null, 'npm test timed out');
  return {
    passed: child.status === 0,
    command: {
      argv: ['npm', 'test'],
      exit: child.status,
      stdout: child.stdout.slice(-5000),
      stderr: child.stderr.slice(-3000),
    },
  };
};

const evaluator = async (policy, suite) => {
  const dir = policy.implementation === 'candidate' ? candidate : baseline;
  const { report, command } = runProbe(dir);
  const native = runNative(dir);
  const wins = suite.id === 'request-boundary-holdout'
    ? report.holdout
    : [native.passed, ...report.anchor];
  measurements.push({ implementation: policy.implementation, suite: suite.id, wins, command, native: native.command });
  const count = wins.filter(Boolean).length;
  return {
    primary: count / wins.length,
    noopRate: (wins.length - count) / wins.length,
    costPerWin: count === 0 ? 1 : 1 / count,
    regressed: suite.id === 'behavior-anchor' && count !== wins.length,
    itemWins: wins,
  };
};

const parentBytes = readFileSync(join(baseline, 'index.js'), 'utf8');
const result = await runFlywheelGenerations({
  rootPolicy: { implementation: 'baseline' },
  mutationTargets: ['implementation'],
  proposer: async () => ({
    value: 'candidate',
    summary: 'Deny browser origins by default, bound request identity, and preserve structured JSON errors.',
    inverse: {
      path: 'index.js',
      parentBytes,
      hash: createHash('sha256').update(parentBytes).digest('hex'),
    },
  }),
  evaluator,
  promotionRule: meetsPromotionRule,
  holdout: { id: 'request-boundary-holdout', items: Array.from({ length: 8 }, (_, i) => i) },
  anchor: { id: 'behavior-anchor', items: Array.from({ length: 5 }, (_, i) => i) },
  maxGenerations: 1,
  signer: makeSigner(),
  now: (generation) => '2026-10-23T00:00:0' + generation + 'Z',
  cacheEvaluations: true,
  dataSource: 'LIVE',
});
const replay = verifyReplayBundle(result.replayBundle);
assert.equal(replay.pass, true, JSON.stringify(replay));
assert.equal(result.promotions.length, 1, 'candidate must pass the frozen promotion gate');

writeFileSync(output, JSON.stringify({
  schema: 1,
  versions: {
    flywheel: packageVersion('@metaharness/flywheel'),
    darwin: packageVersion('@metaharness/darwin'),
    metaharness: packageVersion('metaharness'),
  },
  suites: {
    holdout: [
      'default browser deny',
      'configured origin allow',
      'unconfigured origin deny',
      'correlation response header',
      'correlation body/header consistency',
      'overlong correlation replacement',
      'structured malformed-JSON response',
      'structured missing-content-type response',
    ],
    anchors: [
      'npm test',
      'valid schema remains accepted',
      'unknown route remains structured 404',
      'health response remains available',
      'execution id remains present',
    ],
  },
  measurements,
  liftCurve: result.liftCurve,
  promotions: result.promotions,
  replay,
  replayBundle: result.replayBundle,
  rollback: {
    strategy: 'Revert the candidate commit or restore the signed promotion inverse for index.js.',
    parentSha: process.env.BASELINE_SHA || null,
    inverseSha256: createHash('sha256').update(parentBytes).digest('hex'),
  },
}, null, 2) + '\n');
console.log(JSON.stringify({
  promoted: result.promotions.length,
  replayPass: replay.pass,
  liftCurve: result.liftCurve,
  output,
}));
