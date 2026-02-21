'use strict';

const express = require('express');
const cors = require('cors');
const crypto = require('crypto');

const app = express();

// ── CORS ────────────────────────────────────────────────────────────────────
app.use(cors({
  origin: true,
  methods: ['GET', 'POST', 'OPTIONS'],
  allowedHeaders: ['Content-Type', 'Authorization', 'X-Correlation-ID'],
  exposedHeaders: ['X-Correlation-ID'],
  maxAge: 3600,
}));

// ── Body parsing ────────────────────────────────────────────────────────────
app.use(express.json({ limit: '1mb' }));

// ── Constants ───────────────────────────────────────────────────────────────
const SERVICE_NAME = 'schema-registry-agents';
const SUPPORTED_SCHEMA_TYPES = ['json_schema', 'avro', 'protobuf'];
const MAX_SCHEMA_SIZE = 1024 * 1024; // 1 MB

// ── Helpers ─────────────────────────────────────────────────────────────────

function buildExecutionMetadata(req) {
  return {
    trace_id: req.headers['x-correlation-id'] || crypto.randomUUID(),
    timestamp: new Date().toISOString(),
    service: SERVICE_NAME,
    execution_id: crypto.randomUUID(),
  };
}

function buildResponse(req, data, layers) {
  return {
    execution_metadata: buildExecutionMetadata(req),
    layers_executed: layers,
    ...data,
  };
}

// ── Validate JSON Schema syntax ─────────────────────────────────────────────
function validateJsonSchema(content) {
  const errors = [];
  const warnings = [];
  let parsed;

  try {
    parsed = JSON.parse(content);
  } catch (e) {
    errors.push({ path: '$', message: `Invalid JSON: ${e.message}`, error_type: 'parse_error' });
    return { valid: false, errors, warnings };
  }

  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    errors.push({ path: '$', message: 'Schema must be a JSON object', error_type: 'type_error' });
    return { valid: false, errors, warnings };
  }

  if (!parsed.type && !parsed.$ref && !parsed.oneOf && !parsed.anyOf && !parsed.allOf && !parsed.properties && !parsed.enum) {
    warnings.push({ path: '$', message: 'Schema has no type constraint or composition keyword', warning_type: 'missing_type' });
  }

  return { valid: errors.length === 0, errors, warnings };
}

// ── Validate Avro schema syntax ─────────────────────────────────────────────
function validateAvroSchema(content) {
  const errors = [];
  const warnings = [];
  let parsed;

  try {
    parsed = JSON.parse(content);
  } catch (e) {
    errors.push({ path: '$', message: `Invalid JSON: ${e.message}`, error_type: 'parse_error' });
    return { valid: false, errors, warnings };
  }

  // Avro primitive type as string
  if (typeof parsed === 'string') {
    const primitives = ['null', 'boolean', 'int', 'long', 'float', 'double', 'bytes', 'string'];
    if (!primitives.includes(parsed)) {
      errors.push({ path: '$', message: `Unknown Avro primitive type: ${parsed}`, error_type: 'invalid_type' });
    }
    return { valid: errors.length === 0, errors, warnings };
  }

  // Avro union as array
  if (Array.isArray(parsed)) {
    return { valid: true, errors, warnings };
  }

  if (typeof parsed === 'object' && parsed !== null) {
    if (!parsed.type) {
      errors.push({ path: '$.type', message: 'Avro schema must have a "type" field', error_type: 'missing_field' });
    }
    if (parsed.type === 'record' && !parsed.name) {
      errors.push({ path: '$.name', message: 'Avro record must have a "name" field', error_type: 'missing_field' });
    }
    if (parsed.type === 'record' && !parsed.fields) {
      errors.push({ path: '$.fields', message: 'Avro record must have a "fields" array', error_type: 'missing_field' });
    }
    return { valid: errors.length === 0, errors, warnings };
  }

  errors.push({ path: '$', message: 'Invalid Avro schema structure', error_type: 'type_error' });
  return { valid: false, errors, warnings };
}

// ── Validate Protobuf schema syntax ─────────────────────────────────────────
function validateProtobufSchema(content) {
  const errors = [];
  const warnings = [];

  if (typeof content !== 'string' || content.trim().length === 0) {
    errors.push({ path: '$', message: 'Protobuf schema must be a non-empty string', error_type: 'parse_error' });
    return { valid: false, errors, warnings };
  }

  if (!content.includes('syntax')) {
    warnings.push({ path: '$', message: 'Missing syntax declaration (e.g., syntax = "proto3")', warning_type: 'missing_syntax' });
  }

  if (!content.includes('message') && !content.includes('enum') && !content.includes('service')) {
    errors.push({ path: '$', message: 'Protobuf schema must define at least one message, enum, or service', error_type: 'empty_schema' });
  }

  return { valid: errors.length === 0, errors, warnings };
}

// ── Agent: Schema Validation ────────────────────────────────────────────────
app.post('/v1/schema-registry/validate', (req, res) => {
  const start = Date.now();
  const layers = [
    { layer: 'AGENT_ROUTING', status: 'completed' },
  ];

  const { schema_content, schema_type, strict } = req.body;

  // Input validation
  if (!schema_content) {
    layers.push({ layer: 'SCHEMA_REGISTRY_VALIDATE', status: 'error', duration_ms: Date.now() - start });
    return res.status(400).json(buildResponse(req, {
      error: 'schema_content is required',
    }, layers));
  }

  if (!schema_type) {
    layers.push({ layer: 'SCHEMA_REGISTRY_VALIDATE', status: 'error', duration_ms: Date.now() - start });
    return res.status(400).json(buildResponse(req, {
      error: 'schema_type is required',
    }, layers));
  }

  if (!SUPPORTED_SCHEMA_TYPES.includes(schema_type)) {
    layers.push({ layer: 'SCHEMA_REGISTRY_VALIDATE', status: 'error', duration_ms: Date.now() - start });
    return res.status(400).json(buildResponse(req, {
      error: `Unsupported schema_type: ${schema_type}. Supported: ${SUPPORTED_SCHEMA_TYPES.join(', ')}`,
    }, layers));
  }

  const content = typeof schema_content === 'string' ? schema_content : JSON.stringify(schema_content);

  if (Buffer.byteLength(content, 'utf8') > MAX_SCHEMA_SIZE) {
    layers.push({ layer: 'SCHEMA_REGISTRY_VALIDATE', status: 'error', duration_ms: Date.now() - start });
    return res.status(400).json(buildResponse(req, {
      error: `Schema exceeds maximum size of ${MAX_SCHEMA_SIZE} bytes`,
    }, layers));
  }

  // Dispatch to format-specific validator
  let result;
  switch (schema_type) {
    case 'json_schema':
      result = validateJsonSchema(content);
      break;
    case 'avro':
      result = validateAvroSchema(content);
      break;
    case 'protobuf':
      result = validateProtobufSchema(content);
      break;
  }

  // In strict mode, promote warnings to errors
  if (strict && result.warnings.length > 0) {
    for (const w of result.warnings) {
      result.errors.push({ path: w.path, message: w.message, error_type: w.warning_type });
    }
    result.warnings = [];
    result.valid = result.errors.length === 0;
  }

  const duration_ms = Date.now() - start;
  layers.push({ layer: 'SCHEMA_REGISTRY_VALIDATE', status: 'completed', duration_ms });

  const status = result.valid ? 200 : 422;
  return res.status(status).json(buildResponse(req, {
    valid: result.valid,
    errors: result.errors,
    warnings: result.warnings,
    schema_type,
    validation_time_ms: duration_ms,
  }, layers));
});

// ── Health endpoint ─────────────────────────────────────────────────────────
app.get('/v1/schema-registry/health', (req, res) => {
  res.json(buildResponse(req, {
    status: 'healthy',
    agents: ['validate'],
    uptime_s: Math.floor(process.uptime()),
  }, [
    { layer: 'AGENT_ROUTING', status: 'completed' },
    { layer: 'HEALTH_CHECK', status: 'completed', duration_ms: 0 },
  ]));
});

// ── Catch-all for unknown routes ────────────────────────────────────────────
app.use((req, res) => {
  res.status(404).json(buildResponse(req, {
    error: 'Not found',
    available_routes: [
      'POST /v1/schema-registry/validate',
      'GET  /v1/schema-registry/health',
    ],
  }, [
    { layer: 'AGENT_ROUTING', status: 'error' },
  ]));
});

// ── Export for Cloud Functions ───────────────────────────────────────────────
exports.handler = app;

// ── Local development server ────────────────────────────────────────────────
if (require.main === module) {
  const port = process.env.PORT || 8080;
  app.listen(port, () => {
    console.log(`${SERVICE_NAME} listening on port ${port}`);
  });
}
