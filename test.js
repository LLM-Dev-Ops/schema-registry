'use strict';

const http = require('http');

let passed = 0;
let failed = 0;

function assert(condition, label) {
  if (condition) {
    passed++;
    console.log(`  PASS: ${label}`);
  } else {
    failed++;
    console.error(`  FAIL: ${label}`);
  }
}

async function request(method, path, body) {
  return new Promise((resolve, reject) => {
    const options = {
      hostname: '127.0.0.1',
      port: 9876,
      path,
      method,
      headers: { 'Content-Type': 'application/json' },
    };
    const req = http.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => { data += chunk; });
      res.on('end', () => {
        resolve({ status: res.statusCode, body: JSON.parse(data) });
      });
    });
    req.on('error', reject);
    if (body) req.write(JSON.stringify(body));
    req.end();
  });
}

async function run() {
  const app = require('./index').handler;
  const server = app.listen(9876);

  try {
    // ── Health endpoint ──────────────────────────────────────────────────
    console.log('\nHealth endpoint:');
    const health = await request('GET', '/v1/schema-registry/health');
    assert(health.status === 200, 'health returns 200');
    assert(health.body.status === 'healthy', 'status is healthy');
    assert(Array.isArray(health.body.agents), 'agents is an array');
    assert(health.body.agents.includes('validate'), 'agents includes validate');
    assert(health.body.execution_metadata !== undefined, 'has execution_metadata');
    assert(health.body.execution_metadata.service === 'schema-registry-agents', 'service name correct');
    assert(health.body.execution_metadata.trace_id !== undefined, 'has trace_id');
    assert(health.body.execution_metadata.execution_id !== undefined, 'has execution_id');
    assert(health.body.execution_metadata.timestamp !== undefined, 'has timestamp');
    assert(Array.isArray(health.body.layers_executed), 'has layers_executed');

    // ── Validate: missing body fields ────────────────────────────────────
    console.log('\nValidation - missing fields:');
    const noContent = await request('POST', '/v1/schema-registry/validate', { schema_type: 'json_schema' });
    assert(noContent.status === 400, 'missing schema_content returns 400');
    assert(noContent.body.execution_metadata !== undefined, 'error response has execution_metadata');

    const noType = await request('POST', '/v1/schema-registry/validate', { schema_content: '{}' });
    assert(noType.status === 400, 'missing schema_type returns 400');

    // ── Validate: unsupported type ───────────────────────────────────────
    const badType = await request('POST', '/v1/schema-registry/validate', { schema_content: '{}', schema_type: 'xml' });
    assert(badType.status === 400, 'unsupported schema_type returns 400');

    // ── Validate: valid JSON Schema ──────────────────────────────────────
    console.log('\nValidation - JSON Schema:');
    const validJson = await request('POST', '/v1/schema-registry/validate', {
      schema_content: '{"type":"object","properties":{"name":{"type":"string"}}}',
      schema_type: 'json_schema',
    });
    assert(validJson.status === 200, 'valid JSON schema returns 200');
    assert(validJson.body.valid === true, 'valid flag is true');
    assert(validJson.body.execution_metadata !== undefined, 'has execution_metadata');
    assert(validJson.body.layers_executed.length === 2, 'has 2 layers');
    assert(validJson.body.layers_executed[0].layer === 'AGENT_ROUTING', 'first layer is AGENT_ROUTING');
    assert(validJson.body.layers_executed[1].layer === 'SCHEMA_REGISTRY_VALIDATE', 'second layer is SCHEMA_REGISTRY_VALIDATE');
    assert(typeof validJson.body.layers_executed[1].duration_ms === 'number', 'duration_ms is a number');

    // ── Validate: invalid JSON ───────────────────────────────────────────
    const invalidJson = await request('POST', '/v1/schema-registry/validate', {
      schema_content: '{bad json',
      schema_type: 'json_schema',
    });
    assert(invalidJson.status === 422, 'invalid JSON returns 422');
    assert(invalidJson.body.valid === false, 'valid flag is false');
    assert(invalidJson.body.errors.length > 0, 'has errors');

    // ── Validate: valid Avro ─────────────────────────────────────────────
    console.log('\nValidation - Avro:');
    const validAvro = await request('POST', '/v1/schema-registry/validate', {
      schema_content: '{"type":"record","name":"User","fields":[{"name":"id","type":"int"}]}',
      schema_type: 'avro',
    });
    assert(validAvro.status === 200, 'valid Avro schema returns 200');
    assert(validAvro.body.valid === true, 'valid flag is true');

    const invalidAvro = await request('POST', '/v1/schema-registry/validate', {
      schema_content: '{"name":"NoType"}',
      schema_type: 'avro',
    });
    assert(invalidAvro.status === 422, 'Avro missing type returns 422');

    // ── Validate: valid Protobuf ─────────────────────────────────────────
    console.log('\nValidation - Protobuf:');
    const validProto = await request('POST', '/v1/schema-registry/validate', {
      schema_content: 'syntax = "proto3";\nmessage User { string name = 1; }',
      schema_type: 'protobuf',
    });
    assert(validProto.status === 200, 'valid Protobuf returns 200');
    assert(validProto.body.valid === true, 'valid flag is true');

    const invalidProto = await request('POST', '/v1/schema-registry/validate', {
      schema_content: '// empty proto',
      schema_type: 'protobuf',
    });
    assert(invalidProto.status === 422, 'empty Protobuf returns 422');

    // ── Validate: strict mode ────────────────────────────────────────────
    console.log('\nValidation - strict mode:');
    const strictWarn = await request('POST', '/v1/schema-registry/validate', {
      schema_content: '{}',
      schema_type: 'json_schema',
      strict: true,
    });
    assert(strictWarn.status === 422, 'strict mode promotes warnings to errors');

    // ── Validate: X-Correlation-ID passthrough ───────────────────────────
    console.log('\nCorrelation ID:');
    const corrId = 'test-trace-id-12345';
    const corrReq = await new Promise((resolve, reject) => {
      const options = {
        hostname: '127.0.0.1',
        port: 9876,
        path: '/v1/schema-registry/health',
        method: 'GET',
        headers: { 'X-Correlation-ID': corrId },
      };
      const req = http.request(options, (res) => {
        let data = '';
        res.on('data', (chunk) => { data += chunk; });
        res.on('end', () => resolve({ status: res.statusCode, body: JSON.parse(data) }));
      });
      req.on('error', reject);
      req.end();
    });
    assert(corrReq.body.execution_metadata.trace_id === corrId, 'trace_id matches X-Correlation-ID header');

    // ── 404 ──────────────────────────────────────────────────────────────
    console.log('\n404 handler:');
    const notFound = await request('GET', '/v1/schema-registry/nonexistent');
    assert(notFound.status === 404, 'unknown route returns 404');
    assert(notFound.body.execution_metadata !== undefined, '404 has execution_metadata');
    assert(Array.isArray(notFound.body.available_routes), '404 lists available routes');

    // ── Summary ──────────────────────────────────────────────────────────
    console.log(`\n${'='.repeat(50)}`);
    console.log(`Results: ${passed} passed, ${failed} failed, ${passed + failed} total`);
    console.log(`${'='.repeat(50)}`);
    process.exit(failed > 0 ? 1 : 0);
  } finally {
    server.close();
  }
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});
