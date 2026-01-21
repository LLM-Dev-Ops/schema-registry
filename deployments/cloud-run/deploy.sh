#!/bin/bash
# ============================================================================
# Deploy LLM Schema Registry to Cloud Run
# ============================================================================
#
# ARCHITECTURE: LLM-SCHEMA-REGISTRY is the AUTHORITATIVE CANONICAL SCHEMA SERVICE
# - Serves runtime-accessible, versioned schemas to all agents and services
# - ALL schema endpoints exposed via ONE unified Google Cloud Run service
# - Stateless service - NO direct database connections
# - ALL persistence via ruvector-service only
#
# Usage:
#   ./deploy.sh [dev|staging|prod]
#
# Prerequisites:
#   - gcloud CLI authenticated
#   - Docker configured for Artifact Registry
#   - Required secrets in Secret Manager:
#     - ruvector-api-key: API key for ruvector-service authentication
#   - Service account: llm-schema-registry-sa with required permissions
#
# ============================================================================

set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PROJECT_ID="${GCP_PROJECT_ID:-agentics-dev}"
REGION="${GCP_REGION:-us-central1}"
SERVICE_NAME="llm-schema-registry"
IMAGE_REPO="us-central1-docker.pkg.dev/${PROJECT_ID}/llm-schema-registry/schema-registry"
ENVIRONMENT="${1:-dev}"
TELEMETRY_ENDPOINT="${TELEMETRY_ENDPOINT:-https://llm-observatory.agentics.dev/v1/traces}"

# Validate environment
if [[ ! "$ENVIRONMENT" =~ ^(dev|staging|prod)$ ]]; then
    echo -e "${RED}Error: Invalid environment. Use: dev, staging, or prod${NC}"
    exit 1
fi

echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}Deploying LLM Schema Registry${NC}"
echo -e "${BLUE}==========================================${NC}"
echo "Project:     ${PROJECT_ID}"
echo "Region:      ${REGION}"
echo "Environment: ${ENVIRONMENT}"
echo "Service:     ${SERVICE_NAME}"
echo -e "${BLUE}==========================================${NC}"

# Get the latest image tag (or build new one)
IMAGE_TAG="${IMAGE_TAG:-latest}"
FULL_IMAGE="${IMAGE_REPO}:${IMAGE_TAG}"

echo "Using image: ${FULL_IMAGE}"

# Set environment-specific configuration
case "$ENVIRONMENT" in
    dev)
        MIN_INSTANCES=0
        MAX_INSTANCES=5
        CPU=1
        MEMORY="512Mi"
        RUVECTOR_URL="${RUVECTOR_SERVICE_URL_DEV:-https://ruvector-service-dev.run.app}"
        TRACE_SAMPLING_RATE="0.5"
        ;;
    staging)
        MIN_INSTANCES=1
        MAX_INSTANCES=10
        CPU=2
        MEMORY="1Gi"
        RUVECTOR_URL="${RUVECTOR_SERVICE_URL_STAGING:-https://ruvector-service-staging.run.app}"
        TRACE_SAMPLING_RATE="0.2"
        ;;
    prod)
        MIN_INSTANCES=2
        MAX_INSTANCES=20
        CPU=2
        MEMORY="2Gi"
        RUVECTOR_URL="${RUVECTOR_SERVICE_URL_PROD:-https://ruvector-service.run.app}"
        TRACE_SAMPLING_RATE="0.1"
        ;;
esac

# Verify prerequisites
echo -e "\n${YELLOW}Verifying prerequisites...${NC}"

# Check if service account exists
if ! gcloud iam service-accounts describe "llm-schema-registry-sa@${PROJECT_ID}.iam.gserviceaccount.com" --project="${PROJECT_ID}" &>/dev/null; then
    echo -e "${YELLOW}Creating service account...${NC}"
    gcloud iam service-accounts create llm-schema-registry-sa \
        --display-name="LLM Schema Registry Service Account" \
        --project="${PROJECT_ID}"
fi

# Check if secret exists
if ! gcloud secrets describe ruvector-api-key --project="${PROJECT_ID}" &>/dev/null; then
    echo -e "${RED}Warning: ruvector-api-key secret not found. Create it with:${NC}"
    echo "  gcloud secrets create ruvector-api-key --data-file=./ruvector-api-key.txt --project=${PROJECT_ID}"
fi

# Deploy to Cloud Run
echo -e "\n${YELLOW}Deploying to Cloud Run...${NC}"
gcloud run deploy "${SERVICE_NAME}" \
    --image="${FULL_IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --platform=managed \
    --allow-unauthenticated \
    --port=8080 \
    --cpu="${CPU}" \
    --memory="${MEMORY}" \
    --min-instances="${MIN_INSTANCES}" \
    --max-instances="${MAX_INSTANCES}" \
    --concurrency=80 \
    --timeout=300s \
    --service-account="llm-schema-registry-sa@${PROJECT_ID}.iam.gserviceaccount.com" \
    --set-env-vars="SERVICE_NAME=${SERVICE_NAME}" \
    --set-env-vars="SERVICE_VERSION=${IMAGE_TAG}" \
    --set-env-vars="PLATFORM_ENV=${ENVIRONMENT}" \
    --set-env-vars="RUVECTOR_SERVICE_URL=${RUVECTOR_URL}" \
    --set-env-vars="TELEMETRY_ENDPOINT=${TELEMETRY_ENDPOINT}" \
    --set-env-vars="OTLP_ENDPOINT=${TELEMETRY_ENDPOINT}" \
    --set-env-vars="TRACE_SAMPLING_RATE=${TRACE_SAMPLING_RATE}" \
    --set-env-vars="RUST_LOG=info" \
    --set-env-vars="RUST_BACKTRACE=1" \
    --set-env-vars="JSON_LOGS=true" \
    --set-env-vars="MAX_SCHEMA_SIZE=1048576" \
    --set-env-vars="VALIDATION_TIMEOUT_MS=5000" \
    --set-env-vars="STRICT_MODE=false" \
    --set-env-vars="ENABLE_SEMANTIC_CHECKS=true" \
    --set-env-vars="ENABLE_PERFORMANCE_CHECKS=true" \
    --set-env-vars="ENABLE_SECURITY_CHECKS=true" \
    --set-env-vars="MAX_NESTING_DEPTH=50" \
    --set-env-vars="MAX_PROPERTIES=500" \
    --set-secrets="RUVECTOR_API_KEY=ruvector-api-key:latest" \
    --labels="app=${SERVICE_NAME},team=agentics,component=schema-governance,environment=${ENVIRONMENT}"

# Get the service URL
SERVICE_URL=$(gcloud run services describe "${SERVICE_NAME}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --format="value(status.url)")

# Verify deployment
echo -e "\n${YELLOW}Verifying deployment...${NC}"
sleep 5

# Health check
HEALTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "${SERVICE_URL}/health" || echo "000")
if [[ "$HEALTH_STATUS" == "200" ]]; then
    echo -e "${GREEN}✓ Health check passed (HTTP ${HEALTH_STATUS})${NC}"
else
    echo -e "${RED}✗ Health check failed (HTTP ${HEALTH_STATUS})${NC}"
fi

echo ""
echo -e "${GREEN}==========================================${NC}"
echo -e "${GREEN}Deployment Complete!${NC}"
echo -e "${GREEN}==========================================${NC}"
echo "Service URL: ${SERVICE_URL}"
echo ""
echo "Schema Endpoints:"
echo "  Health:      ${SERVICE_URL}/health"
echo "  Ready:       ${SERVICE_URL}/ready"
echo "  Metrics:     ${SERVICE_URL}/metrics"
echo "  Schemas:     ${SERVICE_URL}/api/v1/schemas"
echo "  Get Schema:  ${SERVICE_URL}/api/v1/schemas/{id}"
echo "  Validate:    ${SERVICE_URL}/api/v1/validate/{id}"
echo "  Compat:      ${SERVICE_URL}/api/v1/compatibility/check"
echo ""
echo "Test commands:"
echo "  curl ${SERVICE_URL}/health"
echo "  curl -X POST ${SERVICE_URL}/api/v1/schemas -H 'Content-Type: application/json' -d '{...}'"
echo ""
echo "CLI commands:"
echo "  schema-registry agent validate --file ./schema.json --namespace com.example --name MySchema"
echo "  schema-registry agent register --file ./schema.json --namespace com.example --name MySchema --version 1.0.0"
echo -e "${GREEN}==========================================${NC}"
