# LLM-SCHEMA-REGISTRY - Final Production Deployment

## Architecture Context

**LLM-SCHEMA-REGISTRY** is the **AUTHORITATIVE CANONICAL SCHEMA SERVICE** for the Agentics platform.

### Responsibilities

- Serve canonical schemas for:
  - DecisionEvents
  - Agent inputs
  - Agent outputs
  - Telemetry
  - Connectors
  - Governance
  - Analytics
- Provide deterministic, read-only schema retrieval
- Enforce schema version resolution rules
- Support runtime and CLI-based schema validation
- Emit schema-access telemetry

### Architectural Constraints

- **DOES NOT** execute workflows
- **DOES NOT** emit DecisionEvents
- **DOES NOT** generate analytics
- **DOES NOT** orchestrate agents
- **DOES NOT** enforce policies
- **DOES NOT** optimize behavior
- **DOES NOT** mutate schemas at runtime

---

## 1. Service Topology

### Unified Service: `llm-schema-registry`

A single Cloud Run service exposing all schema endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Root liveness probe (always returns 200) |
| `/healthz` | GET | Liveness probe (always returns 200) |
| `/readyz` | GET | Readiness probe (mode-aware) |
| `/health` | GET | Full health check with component status |
| `/api/v1/schemas` | POST | Register new schema |
| `/api/v1/schemas/{id}` | GET | Retrieve schema by ID |
| `/api/v1/validate/{id}` | POST | Validate data against schema |
| `/api/v1/compatibility/check` | POST | Check schema compatibility |

### Server Modes

The server supports three operational modes:

| Mode | `SERVER_MODE` | Description | Use Case |
|------|---------------|-------------|----------|
| **MemoryOnly** | `memory` | In-memory storage, no external dependencies | Cloud Run (default) |
| **Serverless** | `serverless` | Memory + RuVector persistence | Cloud Run with persistence |
| **Full** | `full` | PostgreSQL + Redis (traditional) | Self-hosted, VMs |

**Cloud Run Default**: The service auto-detects `MemoryOnly` mode when:
- `DATABASE_URL` is not set
- `RUVECTOR_SERVICE_URL` is not set

This ensures the container starts **immediately** without waiting for database connections.

### Schema Categories Served

| Category | Description | Example Schemas |
|----------|-------------|-----------------|
| DecisionEvent | Agent decision tracking | `decision-event.v1.json` |
| Agent Input | Input validation contracts | `agent-input.v1.json` |
| Agent Output | Output structure contracts | `agent-output.v1.json` |
| Telemetry | Observability data structures | `telemetry-event.v1.json` |
| Connectors | Integration contracts | `connector-config.v1.json` |
| Governance | Policy and compliance | `governance-rule.v1.json` |
| Analytics | Aggregation schemas | `analytics-event.v1.json` |

### Topology Constraints

- No schema endpoint is deployed as a standalone service
- All endpoints share runtime, configuration, and telemetry
- Single container, single service account
- Stateless execution - no local persistence

---

## 2. Environment Configuration

### Required Environment Variables

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `SERVER_MODE` | No | Server operation mode | `memory`, `serverless`, `full` |
| `PORT` | No | HTTP port (Cloud Run sets this) | `8080` |
| `SERVICE_NAME` | Yes | Service identifier | `llm-schema-registry` |
| `SERVICE_VERSION` | Yes | Service version (commit SHA) | `abc123def` |
| `PLATFORM_ENV` | Yes | Environment | `dev`, `staging`, `prod` |
| `RUVECTOR_SERVICE_URL` | No* | RuVector persistence endpoint | `https://ruvector-service.run.app` |
| `RUVECTOR_API_KEY` | No* | RuVector authentication (Secret Manager) | `sm://ruvector-api-key` |
| `TELEMETRY_ENDPOINT` | No | LLM-Observatory traces endpoint | `https://llm-observatory.agentics.dev/v1/traces` |
| `OTLP_ENDPOINT` | No | OpenTelemetry endpoint (alias) | Same as TELEMETRY_ENDPOINT |
| `TRACE_SAMPLING_RATE` | No | Trace sampling rate | `0.1` (prod), `0.5` (dev) |
| `RUST_LOG` | No | Log level | `info` |
| `JSON_LOGS` | No | Enable JSON logging | `true` |

*Required only when `SERVER_MODE=serverless`

### Validation Engine Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `MAX_SCHEMA_SIZE` | `1048576` | Maximum schema size (1MB) |
| `VALIDATION_TIMEOUT_MS` | `5000` | Validation timeout (5s) |
| `STRICT_MODE` | `false` | Enable strict validation |
| `ENABLE_SEMANTIC_CHECKS` | `true` | Semantic validation |
| `ENABLE_PERFORMANCE_CHECKS` | `true` | Performance checks |
| `ENABLE_SECURITY_CHECKS` | `true` | Security checks |
| `MAX_NESTING_DEPTH` | `50` | Maximum schema nesting |
| `MAX_PROPERTIES` | `500` | Maximum properties |

### Validation Rules

- No hardcoded URLs or service names
- No embedded secrets
- All secrets resolved via Secret Manager
- Environment variables validated at startup

---

## 3. Google SQL / Schema Metadata Wiring

### CRITICAL: No Direct Database Connectivity

**CONFIRMED: LLM-Schema-Registry does NOT connect directly to Google SQL (PostgreSQL).**

All schema access logs, version references, and metadata are written via **ruvector-service** only.

### Persistence Flow

```
┌──────────────────────┐      ┌───────────────────┐      ┌──────────────┐
│  LLM-Schema-Registry │ ───> │  ruvector-service │ ───> │  Google SQL  │
│    (Read-Only)       │      │   (Persistence)   │      │  (Postgres)  │
└──────────────────────┘      └───────────────────┘      └──────────────┘
```

### Persistence Guarantees

| Requirement | Implementation |
|-------------|----------------|
| Append-only persistence | All writes are inserts via ruvector-service |
| Idempotent writes | Content hash deduplication |
| Retry safety | Idempotency keys in requests |
| Schema immutability | Schemas served from versioned artifacts |

### What Gets Persisted via ruvector-service

- Schema registration events
- Schema access telemetry
- Version resolution metadata
- Validation results (optional)
- Compatibility check results

### What Does NOT Get Persisted

- Schema content (served from immutable artifacts)
- Runtime validation state
- Temporary computation results

---

## 4. Cloud Build & Deployment

### Cloud Build Pipeline

```bash
# Deploy to dev
gcloud builds submit --config=cloudbuild.yaml \
  --substitutions=_ENVIRONMENT=dev \
  --project=agentics-dev

# Deploy to staging
gcloud builds submit --config=cloudbuild.yaml \
  --substitutions=_ENVIRONMENT=staging,_MIN_INSTANCES=1,_MAX_INSTANCES=10 \
  --project=agentics-dev

# Deploy to production
gcloud builds submit --config=cloudbuild.yaml \
  --substitutions=_ENVIRONMENT=prod,_MIN_INSTANCES=2,_MAX_INSTANCES=20,_MEMORY=2Gi \
  --project=agentics-prod
```

### Using Deploy Script

```bash
# Deploy to dev
./deployments/cloud-run/deploy.sh dev

# Deploy to staging
./deployments/cloud-run/deploy.sh staging

# Deploy to production
./deployments/cloud-run/deploy.sh prod
```

### IAM Service Account Requirements (Least Privilege)

```bash
# Create service account
gcloud iam service-accounts create llm-schema-registry-sa \
  --display-name="LLM Schema Registry Service Account" \
  --project=agentics-dev

# Grant Secret Manager access (for RUVECTOR_API_KEY)
gcloud secrets add-iam-policy-binding ruvector-api-key \
  --member="serviceAccount:llm-schema-registry-sa@agentics-dev.iam.gserviceaccount.com" \
  --role="roles/secretmanager.secretAccessor" \
  --project=agentics-dev

# Grant Artifact Registry reader
gcloud projects add-iam-policy-binding agentics-dev \
  --member="serviceAccount:llm-schema-registry-sa@agentics-dev.iam.gserviceaccount.com" \
  --role="roles/artifactregistry.reader"

# Grant Cloud Run invoker (for service-to-service calls)
gcloud projects add-iam-policy-binding agentics-dev \
  --member="serviceAccount:llm-schema-registry-sa@agentics-dev.iam.gserviceaccount.com" \
  --role="roles/run.invoker"

# Grant Cloud Trace agent (for telemetry)
gcloud projects add-iam-policy-binding agentics-dev \
  --member="serviceAccount:llm-schema-registry-sa@agentics-dev.iam.gserviceaccount.com" \
  --role="roles/cloudtrace.agent"
```

### Networking Constraints

| Direction | Policy |
|-----------|--------|
| Ingress | Controlled via `--allow-unauthenticated` (public endpoints) |
| Egress | Internal only to: ruvector-service, llm-observatory |
| VPC | Optional VPC connector for private networking |

### Container Configuration

| Parameter | Dev | Staging | Prod |
|-----------|-----|---------|------|
| CPU | 1 | 2 | 2 |
| Memory | 512Mi | 1Gi | 2Gi |
| Min Instances | 0 | 1 | 2 |
| Max Instances | 5 | 10 | 20 |
| Concurrency | 80 | 80 | 80 |
| Timeout | 300s | 300s | 300s |

---

## 5. CLI & Agent Integration Verification

### CLI Installation

```bash
# Build CLI from source
cargo build --release --package schema-registry-cli

# Install globally
cargo install --path crates/schema-registry-cli
```

### CLI Schema Fetch Commands

```bash
# Fetch schema by name and version
schema-registry agent inspect \
  --file ./schemas/user-created.json

# Expected success output:
{
  "format": "json-schema",
  "structure": {
    "root_type": "object",
    "total_fields": 5,
    "required_fields": 3,
    "optional_fields": 2,
    "nested_depth": 2,
    "has_recursion": false
  },
  "metadata": {
    "schema_size_bytes": 1234,
    "has_description": true,
    "has_examples": false,
    "llm_readiness_score": 0.65
  }
}
```

### CLI Validation Commands

```bash
# Validate a schema definition
schema-registry agent validate \
  --file ./my-schema.json \
  --namespace com.example \
  --name UserCreated \
  --mode strict \
  --output json

# Expected success output:
{
  "is_valid": true,
  "schema_format": "json-schema",
  "namespace": "com.example",
  "name": "UserCreated",
  "validation_mode": "strict",
  "errors": [],
  "warnings": [],
  "metrics": {
    "duration_ms": 12,
    "rules_applied": 3,
    "fields_validated": 5,
    "schema_size_bytes": 1024
  }
}

# Expected failure output:
{
  "is_valid": false,
  "errors": [
    {
      "rule": "type-validation",
      "message": "Field 'email' has no type definition",
      "severity": "error",
      "location": "$.properties.email",
      "suggestion": "Add a 'type' field"
    }
  ]
}
```

### CLI Registration Commands

```bash
# Register schema with the registry
schema-registry agent register \
  --file ./user-created.json \
  --namespace com.example.events \
  --name UserCreated \
  --version 1.0.0 \
  --ruvector-url https://ruvector-service.run.app \
  --output json

# Expected output:
{
  "success": true,
  "schema_id": "550e8400-e29b-41d4-a716-446655440000",
  "namespace": "com.example.events",
  "name": "UserCreated",
  "version": "1.0.0",
  "content_hash": "abc123...",
  "registered_at": "2026-01-21T10:30:00Z",
  "decision_event": {
    "event_id": "evt-123",
    "event_type": "SchemaRegistered",
    "decision": "APPROVED"
  }
}
```

### Runtime Agent Integration

Agents resolve schema URLs dynamically:

```rust
// Agent schema resolution
let schema_url = format!(
    "{}/api/v1/schemas/{}",
    env::var("SCHEMA_REGISTRY_URL")?,
    schema_id
);

let schema = http_client.get(&schema_url).await?;

// Fail deterministically on schema mismatch
if !validate_against_schema(&data, &schema) {
    return Err(SchemaValidationError::Mismatch {
        expected: schema.version,
        actual: data.schema_version,
    });
}
```

### Schema Update Independence

**Schema updates do NOT require agent redeployment:**

- Agents resolve schemas at runtime via URL
- Version resolution is deterministic (latest or pinned)
- Cache invalidation handled via TTL

---

## 6. Platform & Core Integration

### Consumer Services (Read-Only)

| Service | Schema Usage | Integration Point |
|---------|-------------|-------------------|
| LLM-Connector-Hub | Normalization validation | `/api/v1/validate/{id}` |
| LLM-Analytics-Hub | Aggregation input schemas | `/api/v1/schemas/{id}` |
| LLM-Policy-Engine | Constraint interpretation | `/api/v1/schemas/{id}` |
| LLM-Orchestrator | Agent contract validation | `/api/v1/validate/{id}` |
| Runtime Agents | Input/output contracts | `/api/v1/schemas/{id}` |

### Integration Constraints

| Constraint | Enforcement |
|------------|-------------|
| All services consume schemas read-only | No PUT/DELETE endpoints exposed |
| No inline schema definitions | All schemas fetched from registry |
| No duplicated schema definitions | Single source of truth |
| Deterministic version resolution | Semantic versioning rules |

### What LLM-Schema-Registry DOES NOT Invoke

| Component | Reason |
|-----------|--------|
| Execution pipelines | Not an orchestrator |
| Enforcement layers | Not a policy engine |
| Optimization agents | Not an optimizer |
| Analytics workflows | Not an analytics service |

---

## 7. Post-Deploy Verification Checklist

### Health & Availability

- [ ] Liveness probe works: `curl $SERVICE_URL/` returns HTTP 200
- [ ] Liveness probe (healthz): `curl $SERVICE_URL/healthz` returns HTTP 200
- [ ] Readiness check passes: `curl $SERVICE_URL/readyz` returns HTTP 200
- [ ] Full health check: `curl $SERVICE_URL/health` returns component status
- [ ] Server mode is correct: `/health` response shows `"mode": "MemoryOnly"` (or expected mode)

### API Endpoints

- [ ] Schema registration: `POST /api/v1/schemas` returns HTTP 201
- [ ] Schema retrieval: `GET /api/v1/schemas/{id}` returns HTTP 200
- [ ] Schema validation: `POST /api/v1/validate/{id}` returns validation result
- [ ] Compatibility check: `POST /api/v1/compatibility/check` returns result

### Validation Behavior

- [ ] Valid payloads validate successfully (is_valid: true)
- [ ] Invalid payloads fail consistently (is_valid: false, errors populated)
- [ ] Version resolution is deterministic

### Integration

- [ ] Schema metadata persists via ruvector-service
- [ ] Telemetry visible in LLM-Observatory
- [ ] CLI commands work end-to-end

### Architecture Compliance

- [ ] No direct SQL access (verify in Cloud Logging)
- [ ] No runtime schema mutation
- [ ] Environment variables properly configured
- [ ] Secrets loaded from Secret Manager

### Verification Commands

```bash
# Set service URL
export SERVICE_URL=$(gcloud run services describe llm-schema-registry \
  --region=us-central1 --format='value(status.url)')

# 1. Liveness check (fast, always returns 200)
curl -s $SERVICE_URL/ | jq .

# 2. Readiness check
curl -s $SERVICE_URL/readyz | jq .

# 3. Full health check with component status
curl -s $SERVICE_URL/health | jq .

# 4. Register a test schema
curl -X POST $SERVICE_URL/api/v1/schemas \
  -H "Content-Type: application/json" \
  -d '{
    "subject": "com.example.TestSchema",
    "schema": {"type": "object", "properties": {"name": {"type": "string"}}},
    "schema_type": "JSON"
  }' | jq .

# 5. Retrieve the schema (use the ID from step 4)
SCHEMA_ID="<id-from-step-4>"
curl -s $SERVICE_URL/api/v1/schemas/$SCHEMA_ID | jq .

# 6. Validate data against schema
curl -X POST $SERVICE_URL/api/v1/validate/$SCHEMA_ID \
  -H "Content-Type: application/json" \
  -d '{"name": "test"}' | jq .
```

---

## 8. Failure Modes & Rollback

### Common Failures

| Failure | Detection | Resolution |
|---------|-----------|------------|
| Missing schema version | HTTP 404 on GET | Verify schema was registered |
| Version mismatch | Validation error | Check version format (semver) |
| Endpoint misconfiguration | HTTP 404/405 | Verify route configuration |
| RuVector unavailable | HTTP 503, /ready fails | Check ruvector-service status |
| Timeout | HTTP 504 | Reduce schema complexity or increase timeout |
| Out of memory | Container restart | Increase memory allocation |
| Invalid schema format | HTTP 400 | Fix schema syntax |

### Detection Signals

| Signal | Monitoring | Alert Threshold |
|--------|------------|-----------------|
| Validation failures | Error rate metrics | > 5% error rate |
| Missing schema metadata | Log analysis | Any ERROR level logs |
| Latency degradation | P99 latency | > 500ms |
| Container restarts | Cloud Run metrics | > 2 restarts/hour |

### Rollback Procedure

```bash
# 1. List revisions
gcloud run revisions list \
  --service=llm-schema-registry \
  --region=us-central1

# 2. Identify previous stable revision
# Example output:
#   llm-schema-registry-00003-abc  ACTIVE   100%
#   llm-schema-registry-00002-xyz  RETIRED  0%
#   llm-schema-registry-00001-123  RETIRED  0%

# 3. Rollback to previous revision
gcloud run services update-traffic llm-schema-registry \
  --region=us-central1 \
  --to-revisions=llm-schema-registry-00002-xyz=100

# 4. Verify rollback
curl -s $SERVICE_URL/health
```

### Safe Redeploy Strategy

```bash
# 1. Deploy new version with 0% traffic
gcloud run deploy llm-schema-registry \
  --image=NEW_IMAGE \
  --no-traffic

# 2. Get new revision name
NEW_REVISION=$(gcloud run revisions list \
  --service=llm-schema-registry \
  --region=us-central1 \
  --format='value(metadata.name)' \
  --limit=1)

# 3. Test new revision directly
curl -s https://$NEW_REVISION---llm-schema-registry-xxx.run.app/health

# 4. Gradual traffic shift
gcloud run services update-traffic llm-schema-registry \
  --region=us-central1 \
  --to-revisions=$NEW_REVISION=10

# 5. Monitor for 5 minutes, then increase
gcloud run services update-traffic llm-schema-registry \
  --region=us-central1 \
  --to-revisions=$NEW_REVISION=50

# 6. Full cutover if healthy
gcloud run services update-traffic llm-schema-registry \
  --region=us-central1 \
  --to-latest
```

### Schema Immutability Preservation

**During rollback:**

- Schemas are immutable and served from versioned artifacts
- No schema data is lost during service rollback
- Version resolution remains deterministic
- Registered schemas persist in ruvector-service

---

## Architecture Compliance Summary

### LLM-Schema-Registry DOES:

- ✅ Define canonical schemas for agents, services, DecisionEvents
- ✅ Manage schema versioning, evolution, and compatibility rules
- ✅ Validate schema correctness and structural integrity
- ✅ Detect breaking vs non-breaking schema changes
- ✅ Produce read-only schema artifacts
- ✅ Emit schema-access telemetry to LLM-Observatory

### LLM-Schema-Registry DOES NOT:

- ❌ Connect directly to Google SQL
- ❌ Execute SQL queries
- ❌ Intercept runtime execution
- ❌ Execute workflows (LLM-Orchestrator does this)
- ❌ Enforce runtime policies (LLM-Policy-Engine does this)
- ❌ Optimize configurations (LLM-Auto-Optimizer does this)
- ❌ Generate analytics (LLM-Analytics-Hub does this)
- ❌ Emit DecisionEvents (consumers do this)
- ❌ Mutate schemas at runtime

---

## Deployment Commands Summary

```bash
# Quick deploy to dev
./deployments/cloud-run/deploy.sh dev

# Production deploy with Cloud Build
gcloud builds submit --config=cloudbuild.yaml \
  --substitutions=_ENVIRONMENT=prod,_MIN_INSTANCES=2,_MAX_INSTANCES=20,_MEMORY=2Gi

# Verify deployment
curl $(gcloud run services describe llm-schema-registry \
  --region=us-central1 --format='value(status.url)')/health

# Rollback
gcloud run services update-traffic llm-schema-registry \
  --region=us-central1 \
  --to-revisions=PREVIOUS_REVISION=100
```
