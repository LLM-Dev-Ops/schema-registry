# Schema Validation Agent Contract

**Version:** 1.0.0
**Agent ID:** `schema-validation-agent`
**Last Updated:** 2025-01-21
**Status:** Draft

---

## Table of Contents

1. [Agent Purpose Statement](#1-agent-purpose-statement)
2. [Scope and Boundaries](#2-scope-and-boundaries)
3. [Input Schema References](#3-input-schema-references)
4. [Output Schema References](#4-output-schema-references)
5. [DecisionEvent Mapping](#5-decisionevent-mapping)
6. [Confidence Semantics](#6-confidence-semantics)
7. [CLI Contract](#7-cli-contract)
8. [Explicit NON-Responsibilities](#8-explicit-non-responsibilities)
9. [Failure Modes](#9-failure-modes)
10. [Integration Points](#10-integration-points)
11. [Security Considerations](#11-security-considerations)
12. [Versioning and Compatibility](#12-versioning-and-compatibility)

---

## 1. Agent Purpose Statement

### Primary Mission

The Schema Validation Agent is a **read-only, deterministic validation service** responsible for:

- **SCHEMA VALIDATION**: Validate schema definitions for syntactic correctness and structural integrity
- **CONTRACT INTEGRITY VERIFICATION**: Ensure schemas conform to their declared format specifications
- **STRUCTURAL CHECKING**: Verify schema structure follows format-specific rules (JSON Schema, Avro, Protobuf)
- **SEMANTIC CHECKING**: Validate logical consistency within schema definitions

### Core Principle

> **The Schema Validation Agent validates schema definitions for correctness, completeness, and compatibility WITHOUT modifying or enforcing schema usage.**

This agent operates as a pure function: given identical inputs, it produces identical outputs. It has no side effects on the schema registry state beyond emitting DecisionEvents for audit and observability purposes.

### Value Proposition

| Capability | Description |
|------------|-------------|
| **Format Compliance** | Ensures schemas comply with JSON Schema Draft 7/2019-09/2020-12, Avro, or Protobuf specifications |
| **Early Error Detection** | Catches malformed schemas before registration, preventing downstream failures |
| **Compatibility Analysis** | Assesses schema changes against previous versions for breaking change detection |
| **Audit Trail** | Every validation decision is recorded as an immutable DecisionEvent |
| **Deterministic Results** | Same input always produces same output, enabling reproducible validation |

---

## 2. Scope and Boundaries

### In Scope

| Function | Description |
|----------|-------------|
| Structural validation | Verify schema syntax is well-formed |
| Type validation | Ensure all declared types are valid for the format |
| Semantic validation | Check logical consistency (e.g., required fields exist in properties) |
| Security validation | Detect potentially malicious patterns and complexity attacks |
| Performance validation | Flag schemas that may cause performance issues |
| Compatibility assessment | Compare schemas for breaking changes (read-only) |
| Metrics collection | Gather validation timing and coverage metrics |

### Out of Scope

| Function | Reason |
|----------|--------|
| Schema modification | Validation is read-only by design |
| Schema storage | Storage is handled by schema-registry-storage |
| Runtime enforcement | Enforcement is handled by application code |
| Deployment decisions | Deployment is handled by CI/CD pipelines |
| Data transformation | Transformation is handled by schema-registry-migration |

---

## 3. Input Schema References

### 3.1 ValidationRequest

The primary input for schema validation operations.

```typescript
interface ValidationRequest {
  // Schema content to validate
  content: string;

  // Schema format identifier
  format: "JSON_SCHEMA" | "AVRO" | "PROTOBUF";

  // Schema metadata
  metadata: SchemaMetadata;

  // Validation mode
  mode: ValidationMode;

  // Optional: Target version for compatibility checking
  compatibility_target?: SchemaVersion;

  // Request tracking
  request_id: string;
  timestamp: string; // ISO 8601 UTC
}

interface SchemaMetadata {
  // Unique namespace for schema organization
  namespace: string;

  // Schema name within namespace
  name: string;

  // Optional version (for version-specific validation)
  version?: SemanticVersion;

  // Optional description
  description?: string;

  // Custom metadata key-value pairs
  custom?: Record<string, string>;
}

interface SemanticVersion {
  major: number;
  minor: number;
  patch: number;
  prerelease?: string;
  build_metadata?: string;
}

enum ValidationMode {
  // Full validation with all checks enabled
  STRICT = "STRICT",

  // Lenient validation (warnings instead of errors for non-critical issues)
  LENIENT = "LENIENT",

  // Syntax-only validation (fastest)
  SYNTAX_ONLY = "SYNTAX_ONLY",

  // Compatibility-focused validation
  COMPATIBILITY_CHECK = "COMPATIBILITY_CHECK"
}
```

### 3.2 CompatibilityCheckRequest

Input for schema compatibility assessment.

```typescript
interface CompatibilityCheckRequest {
  // New schema to validate
  new_schema: {
    content: string;
    format: SchemaFormat;
    metadata: SchemaMetadata;
  };

  // Previous schema(s) to check against
  previous_schemas: Array<{
    content: string;
    version: SemanticVersion;
  }>;

  // Compatibility mode to enforce
  compatibility_mode: CompatibilityMode;

  // Request tracking
  request_id: string;
  timestamp: string;
}

enum CompatibilityMode {
  BACKWARD = "BACKWARD",           // New can read old data
  FORWARD = "FORWARD",             // Old can read new data
  FULL = "FULL",                   // Both directions
  BACKWARD_TRANSITIVE = "BACKWARD_TRANSITIVE",
  FORWARD_TRANSITIVE = "FORWARD_TRANSITIVE",
  FULL_TRANSITIVE = "FULL_TRANSITIVE",
  NONE = "NONE"                    // No compatibility required
}
```

### 3.3 Supported Schema Formats

#### JSON Schema

- **Supported Drafts**: Draft-07, Draft 2019-09, Draft 2020-12
- **Content-Type**: `application/schema+json`
- **Detection**: Presence of `$schema`, `$id`, or JSON Schema keywords

#### Apache Avro

- **Supported Versions**: 1.8.x - 1.11.x specification
- **Content-Type**: `application/avro+json`
- **Detection**: Presence of `type`, `name`, `fields` for records

#### Protocol Buffers

- **Supported Versions**: proto2, proto3
- **Content-Type**: `application/x-protobuf`
- **Detection**: Presence of `syntax = "proto2"` or `syntax = "proto3"`

---

## 4. Output Schema References

### 4.1 ValidationResult

The primary output from schema validation operations.

```typescript
interface ValidationResult {
  // Overall validation status
  is_valid: boolean;

  // Validation errors (blocking issues)
  errors: ValidationError[];

  // Validation warnings (non-blocking issues)
  warnings: ValidationWarning[];

  // Validation metrics
  metrics: ValidationMetrics;

  // Confidence score for the validation result
  confidence: number; // 0.0 - 1.0

  // Correlation with request
  request_id: string;

  // Timestamp of validation
  validated_at: string; // ISO 8601 UTC
}

interface ValidationError {
  // Error code for programmatic handling
  code: string;

  // Human-readable error message
  message: string;

  // JSON path to the problematic location
  field_path?: string;

  // Line number (if available)
  line?: number;

  // Column number (if available)
  column?: number;

  // Error severity
  severity: "ERROR" | "CRITICAL";

  // Validation rule that triggered the error
  rule: string;

  // Suggested fix (if available)
  suggestion?: string;

  // Additional context
  context?: Record<string, unknown>;
}

interface ValidationWarning {
  // Warning code
  code: string;

  // Human-readable warning message
  message: string;

  // JSON path to the relevant location
  field_path?: string;

  // Warning severity
  severity: "WARNING" | "INFO";

  // Validation rule that triggered the warning
  rule: string;
}

interface ValidationMetrics {
  // Total validation duration in milliseconds
  duration_ms: number;

  // Number of validation rules applied
  rules_applied: number;

  // Number of fields validated
  fields_validated: number;

  // Schema size in bytes
  schema_size_bytes: number;

  // Maximum recursion depth encountered
  max_recursion_depth: number;

  // Schema complexity score (1-100)
  complexity_score: number;
}
```

### 4.2 CompatibilityResult

Output from compatibility checking operations.

```typescript
interface CompatibilityResult {
  // Overall compatibility status
  is_compatible: boolean;

  // Compatibility mode that was checked
  mode: CompatibilityMode;

  // List of compatibility violations
  violations: CompatibilityViolation[];

  // Breaking changes detected
  breaking_changes: BreakingChange[];

  // Versions that were compared
  checked_versions: SemanticVersion[];

  // Confidence score
  confidence: number;

  // Recommended version bump
  recommended_version_bump?: "MAJOR" | "MINOR" | "PATCH";
}

interface CompatibilityViolation {
  // Type of violation
  violation_type: ViolationType;

  // Path to the affected field
  field_path: string;

  // Value in old schema
  old_value?: unknown;

  // Value in new schema
  new_value?: unknown;

  // Severity of violation
  severity: "BREAKING" | "WARNING" | "INFO";

  // Human-readable description
  description: string;

  // Suggested migration path
  migration_hint?: string;
}

interface BreakingChange {
  // Type of breaking change
  change_type: string;

  // Affected field path
  field_path: string;

  // Description of the breaking change
  description: string;

  // Impact assessment
  impact: "HIGH" | "MEDIUM" | "LOW";

  // Number of potential consumers affected (if known)
  affected_consumers?: number;
}

enum ViolationType {
  FIELD_REMOVED = "FIELD_REMOVED",
  TYPE_CHANGED = "TYPE_CHANGED",
  REQUIRED_ADDED = "REQUIRED_ADDED",
  CONSTRAINT_ADDED = "CONSTRAINT_ADDED",
  ENUM_VALUE_REMOVED = "ENUM_VALUE_REMOVED",
  FORMAT_CHANGED = "FORMAT_CHANGED",
  DEFAULT_REMOVED = "DEFAULT_REMOVED",
  NULLABLE_CHANGED = "NULLABLE_CHANGED"
}
```

---

## 5. DecisionEvent Mapping

Every validation decision emitted by this agent MUST be recorded as a DecisionEvent for audit, observability, and reproducibility purposes.

### 5.1 DecisionEvent Structure

```typescript
interface DecisionEvent {
  // Unique identifier for this agent
  agent_id: "schema-validation-agent";

  // Semantic version of the agent
  agent_version: string; // e.g., "1.0.0"

  // Type of decision made
  decision_type: DecisionType;

  // SHA-256 hash of canonical inputs
  inputs_hash: string;

  // Decision outputs
  outputs: ValidationResult | CompatibilityResult;

  // Confidence level of the decision
  confidence: number; // 0.0 - 1.0

  // Constraints that were applied
  constraints_applied: Constraint[];

  // Unique execution reference
  execution_ref: string; // UUID v4

  // UTC timestamp in ISO 8601 format
  timestamp: string; // e.g., "2025-01-21T10:30:00.000Z"

  // Optional: Correlation ID for request tracing
  correlation_id?: string;

  // Optional: Parent decision that triggered this one
  parent_decision_ref?: string;
}

enum DecisionType {
  // Schema validation completed
  SCHEMA_VALIDATION_RESULT = "schema_validation_result",

  // Compatibility check completed
  COMPATIBILITY_STATUS = "compatibility_status",

  // Breaking change detected
  BREAKING_CHANGE_FLAG = "breaking_change_flag"
}

interface Constraint {
  // Constraint identifier
  id: string;

  // Constraint type
  type: "schema_scope" | "version_bounds" | "format_restriction" | "complexity_limit";

  // Constraint value/configuration
  value: unknown;

  // Whether the constraint was satisfied
  satisfied: boolean;
}
```

### 5.2 DecisionEvent Examples

#### Schema Validation Decision

```json
{
  "agent_id": "schema-validation-agent",
  "agent_version": "1.0.0",
  "decision_type": "schema_validation_result",
  "inputs_hash": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "outputs": {
    "is_valid": true,
    "errors": [],
    "warnings": [
      {
        "code": "MISSING_DESCRIPTION",
        "message": "Schema lacks description for LLM understanding",
        "severity": "INFO",
        "rule": "llm-description-check"
      }
    ],
    "metrics": {
      "duration_ms": 12.5,
      "rules_applied": 15,
      "fields_validated": 8,
      "schema_size_bytes": 1024,
      "max_recursion_depth": 3,
      "complexity_score": 25
    },
    "confidence": 0.95
  },
  "confidence": 0.95,
  "constraints_applied": [
    {
      "id": "max-schema-size",
      "type": "complexity_limit",
      "value": 1048576,
      "satisfied": true
    },
    {
      "id": "json-schema-draft",
      "type": "format_restriction",
      "value": ["draft-07", "draft-2019-09", "draft-2020-12"],
      "satisfied": true
    }
  ],
  "execution_ref": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2025-01-21T10:30:00.000Z",
  "correlation_id": "req-abc123"
}
```

#### Breaking Change Detection Decision

```json
{
  "agent_id": "schema-validation-agent",
  "agent_version": "1.0.0",
  "decision_type": "breaking_change_flag",
  "inputs_hash": "sha256:abc123...",
  "outputs": {
    "is_compatible": false,
    "mode": "BACKWARD",
    "violations": [
      {
        "violation_type": "FIELD_REMOVED",
        "field_path": "/properties/legacy_id",
        "old_value": {"type": "string"},
        "severity": "BREAKING",
        "description": "Required field 'legacy_id' was removed",
        "migration_hint": "Add default value or make field optional before removal"
      }
    ],
    "breaking_changes": [
      {
        "change_type": "required_field_removal",
        "field_path": "/properties/legacy_id",
        "description": "Removing required field breaks backward compatibility",
        "impact": "HIGH"
      }
    ],
    "checked_versions": [
      {"major": 1, "minor": 0, "patch": 0},
      {"major": 1, "minor": 1, "patch": 0}
    ],
    "confidence": 1.0,
    "recommended_version_bump": "MAJOR"
  },
  "confidence": 1.0,
  "constraints_applied": [
    {
      "id": "compat-mode",
      "type": "version_bounds",
      "value": "BACKWARD",
      "satisfied": false
    }
  ],
  "execution_ref": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2025-01-21T10:31:00.000Z"
}
```

### 5.3 Input Hash Computation

The `inputs_hash` field MUST be computed as follows:

```python
import hashlib
import json

def compute_inputs_hash(request: dict) -> str:
    """
    Compute SHA-256 hash of canonical inputs.

    Canonicalization rules:
    1. Sort all object keys alphabetically
    2. Remove whitespace from JSON
    3. Use UTF-8 encoding
    """
    # Extract relevant input fields
    canonical_input = {
        "content": request["content"],
        "format": request["format"],
        "metadata": {
            "namespace": request["metadata"]["namespace"],
            "name": request["metadata"]["name"],
            "version": request["metadata"].get("version")
        },
        "mode": request["mode"]
    }

    # Canonicalize and hash
    canonical_json = json.dumps(canonical_input, sort_keys=True, separators=(',', ':'))
    hash_bytes = hashlib.sha256(canonical_json.encode('utf-8')).hexdigest()

    return f"sha256:{hash_bytes}"
```

---

## 6. Confidence Semantics

The confidence score represents the agent's certainty in its validation decision. This metric is critical for downstream systems to make informed decisions about schema acceptance.

### 6.1 Confidence Scale

| Score Range | Classification | Description |
|-------------|----------------|-------------|
| **1.0** | Absolute Certainty | All validation rules passed with full coverage; no ambiguity |
| **0.95 - 0.99** | High Confidence | Minor warnings present; validation is thorough |
| **0.80 - 0.94** | Good Confidence | Some rules skipped due to format limitations; high certainty on executed rules |
| **0.50 - 0.79** | Partial Confidence | Significant rules could not be executed; validation gaps exist |
| **0.25 - 0.49** | Low Confidence | Major validation gaps; result should be treated cautiously |
| **< 0.25** | Very Low Confidence | Insufficient data or rules to make a reliable decision |

### 6.2 Confidence Calculation Rules

```typescript
function calculateConfidence(result: ValidationResult): number {
  let confidence = 1.0;

  // Deductions for errors
  if (result.errors.length > 0) {
    // Critical errors significantly reduce confidence
    const criticalCount = result.errors.filter(e => e.severity === "CRITICAL").length;
    const errorCount = result.errors.filter(e => e.severity === "ERROR").length;

    confidence -= criticalCount * 0.15;
    confidence -= errorCount * 0.05;
  }

  // Deductions for skipped rules
  const expectedRules = getExpectedRulesForFormat(result.format);
  const rulesCoverage = result.metrics.rules_applied / expectedRules;
  if (rulesCoverage < 1.0) {
    confidence -= (1.0 - rulesCoverage) * 0.3;
  }

  // Deductions for complexity exceeding safe thresholds
  if (result.metrics.complexity_score > 80) {
    confidence -= 0.1;
  }

  // Deductions for warnings (minor)
  confidence -= result.warnings.length * 0.01;

  // Floor at 0.0
  return Math.max(0.0, confidence);
}
```

### 6.3 Confidence in Compatibility Decisions

| Scenario | Confidence |
|----------|------------|
| All versions compared, no violations | 1.0 |
| All versions compared, warnings only | 0.9 - 0.99 |
| Partial version comparison (some versions unavailable) | 0.7 - 0.89 |
| Breaking change detected with clear violation | 1.0 (for the detection itself) |
| Ambiguous type coercion detected | 0.6 - 0.8 |
| Unable to parse one or more schemas | 0.3 - 0.5 |

---

## 7. CLI Contract

The Schema Validation Agent exposes a command-line interface for direct invocation and integration into CI/CD pipelines.

### 7.1 Commands

#### `register` - Validate and prepare schema for registration

```bash
schema-validation-agent register \
  --namespace <NAMESPACE> \
  --name <NAME> \
  --version <VERSION> \
  --format <json-schema|avro|protobuf> \
  --file <PATH> \
  [--mode <strict|lenient|syntax-only>] \
  [--output <json|yaml|table>]
```

**Options:**
| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--namespace` | Yes | - | Schema namespace |
| `--name` | Yes | - | Schema name |
| `--version` | Yes | - | Schema version (semver) |
| `--format` | Yes | - | Schema format |
| `--file` | Yes | - | Path to schema file |
| `--mode` | No | strict | Validation mode |
| `--output` | No | table | Output format |

**Exit Codes:**
| Code | Meaning |
|------|---------|
| 0 | Validation passed |
| 1 | Validation failed (errors) |
| 2 | Validation passed with warnings |
| 3 | Invalid arguments |
| 4 | File not found |
| 5 | Internal error |

#### `validate` - Validate schema without registration context

```bash
schema-validation-agent validate \
  --file <PATH> \
  [--format <json-schema|avro|protobuf|auto>] \
  [--mode <strict|lenient|syntax-only>] \
  [--output <json|yaml|table>]
```

**Options:**
| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--file` | Yes | - | Path to schema file (or `-` for stdin) |
| `--format` | No | auto | Schema format (auto-detect if not specified) |
| `--mode` | No | strict | Validation mode |
| `--output` | No | table | Output format |

#### `inspect` - Detailed schema analysis

```bash
schema-validation-agent inspect \
  --file <PATH> \
  [--format <json-schema|avro|protobuf|auto>] \
  [--output <json|yaml|table>]
```

**Output includes:**
- Schema structure tree
- Field types and constraints
- Complexity metrics
- Security analysis
- LLM-readiness score

#### `diff` - Compare two schemas

```bash
schema-validation-agent diff \
  --old <PATH> \
  --new <PATH> \
  [--format <json-schema|avro|protobuf|auto>] \
  [--compatibility <backward|forward|full|none>] \
  [--output <json|yaml|table>]
```

**Options:**
| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--old` | Yes | - | Path to old schema |
| `--new` | Yes | - | Path to new schema |
| `--format` | No | auto | Schema format |
| `--compatibility` | No | backward | Compatibility mode to check |
| `--output` | No | table | Output format |

### 7.2 Output Formats

#### JSON Output (`--output json`)

```json
{
  "status": "valid",
  "is_valid": true,
  "errors": [],
  "warnings": [
    {
      "code": "MISSING_DESCRIPTION",
      "message": "Schema lacks root description",
      "field_path": "$",
      "severity": "INFO"
    }
  ],
  "metrics": {
    "duration_ms": 15.2,
    "rules_applied": 12,
    "complexity_score": 35
  },
  "confidence": 0.95
}
```

#### YAML Output (`--output yaml`)

```yaml
status: valid
is_valid: true
errors: []
warnings:
  - code: MISSING_DESCRIPTION
    message: Schema lacks root description
    field_path: "$"
    severity: INFO
metrics:
  duration_ms: 15.2
  rules_applied: 12
  complexity_score: 35
confidence: 0.95
```

#### Table Output (`--output table`)

```
+------------------+-----------------------------------------------+
| VALIDATION RESULT                                               |
+------------------+-----------------------------------------------+
| Status           | VALID                                         |
| Confidence       | 95%                                           |
| Duration         | 15.2ms                                        |
| Rules Applied    | 12                                            |
| Complexity       | 35/100                                        |
+------------------+-----------------------------------------------+

WARNINGS (1)
+--------------------+------------------------------+----------+------+
| Code               | Message                      | Path     | Sev  |
+--------------------+------------------------------+----------+------+
| MISSING_DESCRIPTION| Schema lacks root description| $        | INFO |
+--------------------+------------------------------+----------+------+
```

---

## 8. Explicit NON-Responsibilities

The Schema Validation Agent is explicitly prohibited from performing the following actions. These boundaries are enforced at the code level and audited.

### 8.1 MUST NEVER Do

| Prohibited Action | Reason | Enforcement |
|-------------------|--------|-------------|
| **Modify schemas** | Agent is read-only by design | No write permissions to storage |
| **Auto-correct definitions** | Modifications must be explicit and human-approved | No mutation functions exposed |
| **Enforce schema usage** | Enforcement is application responsibility | No runtime hooks |
| **Block deployments** | Deployment decisions are CI/CD responsibility | Returns advisory results only |
| **Influence runtime execution** | Runtime behavior is application domain | No process control |
| **Execute workflows** | Workflows are orchestrator responsibility | Single-purpose validation only |
| **Modify runtime behavior** | Side-effect free design | Stateless operation |
| **Connect directly to databases** | Database access via storage layer only | No database credentials |
| **Execute SQL** | All data access through typed interfaces | No raw query capability |
| **Store credentials** | Security concern | No credential storage |
| **Make network calls** | Validation is local | Network access disabled |
| **Access filesystem beyond input** | Security sandboxing | Restricted file access |
| **Spawn subprocesses** | Process isolation | No subprocess capability |

### 8.2 Boundary Enforcement

```typescript
// Compile-time enforcement via type system
interface SchemaValidationAgent {
  // Read-only validation operations ONLY
  validate(request: ValidationRequest): Promise<ValidationResult>;
  checkCompatibility(request: CompatibilityCheckRequest): Promise<CompatibilityResult>;
  inspect(schema: string, format: SchemaFormat): Promise<InspectionResult>;

  // NO mutation methods exist
  // NO storage methods exist
  // NO network methods exist
  // NO subprocess methods exist
}

// Runtime enforcement via capability model
const agentCapabilities = {
  read: true,
  write: false,
  network: false,
  filesystem: "input-only",
  subprocess: false,
  database: false,
  credentials: false
};
```

### 8.3 Delegation Table

| Responsibility | Delegated To | Interface |
|----------------|--------------|-----------|
| Schema storage | schema-registry-storage | SchemaStorage trait |
| Schema modification | schema-registry-core | Schema mutation API |
| Deployment enforcement | CI/CD pipeline | Exit codes |
| Runtime validation | Application code | SDK integration |
| Data transformation | schema-registry-migration | Migration API |
| Access control | schema-registry-security | RBAC/ABAC |

---

## 9. Failure Modes

The agent handles failures gracefully with well-defined error responses and recovery behaviors.

### 9.1 Input Validation Failures

| Error Code | Condition | Response | HTTP Status |
|------------|-----------|----------|-------------|
| `INVALID_FORMAT` | Schema format cannot be determined | Return error with format hints | 400 |
| `PARSE_ERROR` | Schema content is not valid JSON/YAML | Return parse error with line/column | 400 |
| `SCHEMA_TOO_LARGE` | Schema exceeds size limit (1MB default) | Return size error with limit | 413 |
| `EMPTY_CONTENT` | Schema content is empty | Return error | 400 |
| `INVALID_ENCODING` | Content is not valid UTF-8 | Return encoding error | 400 |

### 9.2 Schema Parsing Failures

| Error Code | Condition | Response |
|------------|-----------|----------|
| `JSON_SCHEMA_INVALID` | JSON Schema structure invalid | Detailed validation errors |
| `AVRO_SCHEMA_INVALID` | Avro schema structure invalid | Avro-specific error messages |
| `PROTOBUF_SYNTAX_ERROR` | Protobuf syntax error | Line/column with message |
| `RECURSIVE_DEPTH_EXCEEDED` | Schema recursion too deep | Security error with depth limit |
| `UNDEFINED_REFERENCE` | $ref points to non-existent definition | Reference resolution error |

### 9.3 Missing Required Fields

| Error Code | Condition | Required Action |
|------------|-----------|-----------------|
| `MISSING_NAMESPACE` | Namespace not provided for register | Client must provide |
| `MISSING_NAME` | Name not provided for register | Client must provide |
| `MISSING_VERSION` | Version not provided for register | Client must provide |
| `MISSING_FORMAT` | Format cannot be auto-detected | Client must specify |

### 9.4 Version Resolution Failures

| Error Code | Condition | Response |
|------------|-----------|----------|
| `VERSION_NOT_FOUND` | Requested version does not exist | 404 with available versions |
| `VERSION_RANGE_INVALID` | Version range syntax invalid | 400 with syntax help |
| `NO_PREVIOUS_VERSION` | Compatibility check with no history | Warning, not error |

### 9.5 Network/Service Failures

| Error Code | Condition | Recovery |
|------------|-----------|----------|
| `RUVECTOR_UNAVAILABLE` | ruvector-service not reachable | Return validation without vector features |
| `TIMEOUT` | Validation exceeds time limit | Return partial result with confidence reduction |
| `INTERNAL_ERROR` | Unexpected agent error | Log, return 500 with correlation ID |

### 9.6 Error Response Format

```typescript
interface ErrorResponse {
  // Error classification
  error: {
    code: string;
    message: string;
    details?: string;
  };

  // Error context
  context: {
    request_id: string;
    timestamp: string;
    field_path?: string;
    line?: number;
    column?: number;
  };

  // Recovery hints
  hints?: string[];

  // Documentation link
  docs_url?: string;
}
```

---

## 10. Integration Points

### 10.1 Upstream Dependencies

| Service | Purpose | Protocol | Failure Mode |
|---------|---------|----------|--------------|
| schema-registry-core | Type definitions | Library | Fatal |
| ruvector-service | Vector embeddings for semantic validation | gRPC | Graceful degradation |

### 10.2 Downstream Consumers

| Consumer | Consumption Mode | SLA |
|----------|------------------|-----|
| schema-registry-api | Synchronous validation | <50ms p95 |
| schema-registry-storage | Pre-persist validation | <100ms p95 |
| CI/CD pipelines | CLI invocation | <5s total |
| LLM integrations | SDK calls | <100ms p95 |

### 10.3 Event Publishing

The agent publishes DecisionEvents to:

| Channel | Purpose | Format |
|---------|---------|--------|
| `schema.validation.completed` | Audit trail | JSON DecisionEvent |
| `schema.compatibility.checked` | Compatibility decisions | JSON DecisionEvent |
| `schema.breaking-change.detected` | Alerting | JSON DecisionEvent |

---

## 11. Security Considerations

### 11.1 Input Sanitization

- All schema content is validated for UTF-8 encoding
- Maximum schema size enforced (default: 1MB)
- Maximum recursion depth enforced (default: 100)
- ReDoS protection in regex validation

### 11.2 Complexity Attack Prevention

```typescript
interface SecurityLimits {
  max_schema_size_bytes: 1048576;      // 1MB
  max_recursion_depth: 100;
  max_properties_count: 1000;
  max_array_items_validation: 10000;
  max_pattern_length: 1000;
  regex_timeout_ms: 100;
}
```

### 11.3 Audit Requirements

- All DecisionEvents are immutable
- All inputs are hashed for reproducibility
- Execution references enable correlation
- No PII in validation responses

---

## 12. Versioning and Compatibility

### 12.1 Agent Versioning

The agent follows semantic versioning:

- **MAJOR**: Breaking changes to input/output schemas or CLI interface
- **MINOR**: New validation rules, new output fields (backward compatible)
- **PATCH**: Bug fixes, performance improvements

### 12.2 API Compatibility

| API Version | Status | Support Until |
|-------------|--------|---------------|
| v1 | Current | TBD |

### 12.3 DecisionEvent Schema Evolution

DecisionEvents are append-only. New fields may be added in minor versions. Fields are never removed or renamed.

---

## Appendix A: Validation Rules Reference

### A.1 Structural Rules

| Rule ID | Description | Formats |
|---------|-------------|---------|
| `STRUCT-001` | Valid JSON/YAML syntax | All |
| `STRUCT-002` | Required root properties present | JSON Schema, Avro |
| `STRUCT-003` | Valid proto syntax | Protobuf |

### A.2 Type Rules

| Rule ID | Description | Formats |
|---------|-------------|---------|
| `TYPE-001` | Valid type declarations | All |
| `TYPE-002` | Consistent type usage | All |
| `TYPE-003` | No circular type references | All |

### A.3 Semantic Rules

| Rule ID | Description | Formats |
|---------|-------------|---------|
| `SEM-001` | Required fields exist in properties | JSON Schema |
| `SEM-002` | Default values match declared types | All |
| `SEM-003` | Enum values are unique | All |

### A.4 Security Rules

| Rule ID | Description | Formats |
|---------|-------------|---------|
| `SEC-001` | No ReDoS-vulnerable patterns | JSON Schema |
| `SEC-002` | Recursion depth within limits | All |
| `SEC-003` | No external $ref (configurable) | JSON Schema |

### A.5 LLM-Specific Rules

| Rule ID | Description | Formats |
|---------|-------------|---------|
| `LLM-001` | Root description present | All |
| `LLM-002` | Field descriptions present | All |
| `LLM-003` | Examples provided | All |

---

## Appendix B: Error Code Reference

| Code | Category | Description |
|------|----------|-------------|
| `E1001` | Parse | Invalid JSON syntax |
| `E1002` | Parse | Invalid YAML syntax |
| `E1003` | Parse | Invalid Protobuf syntax |
| `E2001` | Structure | Missing required property |
| `E2002` | Structure | Invalid property type |
| `E3001` | Semantic | Circular reference detected |
| `E3002` | Semantic | Required field not in properties |
| `E4001` | Security | ReDoS pattern detected |
| `E4002` | Security | Recursion limit exceeded |
| `E5001` | Compatibility | Breaking field removal |
| `E5002` | Compatibility | Type change detected |

---

## Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0.0 | 2025-01-21 | System Architect | Initial contract definition |
