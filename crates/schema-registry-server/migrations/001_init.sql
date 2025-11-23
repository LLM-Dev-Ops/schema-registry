-- Initial schema for LLM Schema Registry
-- PostgreSQL 14+

-- Create schemas table
CREATE TABLE IF NOT EXISTS schemas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    namespace VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    version_major INT NOT NULL,
    version_minor INT NOT NULL,
    version_patch INT NOT NULL,
    format VARCHAR(50) NOT NULL CHECK (format IN ('JSON', 'AVRO', 'PROTOBUF')),
    content TEXT NOT NULL,
    content_hash CHAR(64) NOT NULL UNIQUE,
    state VARCHAR(50) NOT NULL DEFAULT 'DRAFT' CHECK (state IN ('DRAFT', 'ACTIVE', 'DEPRECATED', 'ARCHIVED', 'DELETED')),
    compatibility_mode VARCHAR(50) NOT NULL DEFAULT 'BACKWARD',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255),
    metadata JSONB DEFAULT '{}',
    tags TEXT[] DEFAULT ARRAY[]::TEXT[],
    description TEXT,
    UNIQUE(namespace, name, version_major, version_minor, version_patch)
);

-- Create index on namespace and name for faster lookups
CREATE INDEX idx_schemas_namespace_name ON schemas(namespace, name);
CREATE INDEX idx_schemas_content_hash ON schemas(content_hash);
CREATE INDEX idx_schemas_state ON schemas(state);
CREATE INDEX idx_schemas_created_at ON schemas(created_at DESC);
CREATE INDEX idx_schemas_tags ON schemas USING GIN(tags);
CREATE INDEX idx_schemas_metadata ON schemas USING GIN(metadata);

-- Create schema_versions view for easy version listing
CREATE OR REPLACE VIEW schema_versions AS
SELECT
    id,
    namespace,
    name,
    CONCAT(version_major, '.', version_minor, '.', version_patch) as version,
    format,
    state,
    created_at
FROM schemas
ORDER BY namespace, name, version_major DESC, version_minor DESC, version_patch DESC;

-- Create compatibility_checks table
CREATE TABLE IF NOT EXISTS compatibility_checks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schema_id UUID NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    compared_schema_id UUID NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    compatibility_mode VARCHAR(50) NOT NULL,
    is_compatible BOOLEAN NOT NULL,
    violations JSONB DEFAULT '[]',
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(schema_id, compared_schema_id, compatibility_mode)
);

CREATE INDEX idx_compat_schema_id ON compatibility_checks(schema_id);
CREATE INDEX idx_compat_compared_schema_id ON compatibility_checks(compared_schema_id);

-- Create validation_results table
CREATE TABLE IF NOT EXISTS validation_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schema_id UUID NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    data_hash CHAR(64) NOT NULL,
    is_valid BOOLEAN NOT NULL,
    errors JSONB DEFAULT '[]',
    warnings JSONB DEFAULT '[]',
    validated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_validation_schema_id ON validation_results(schema_id);
CREATE INDEX idx_validation_data_hash ON validation_results(data_hash);

-- Create schema_events table for event sourcing
CREATE TABLE IF NOT EXISTS schema_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schema_id UUID NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255)
);

CREATE INDEX idx_events_schema_id ON schema_events(schema_id);
CREATE INDEX idx_events_type ON schema_events(event_type);
CREATE INDEX idx_events_created_at ON schema_events(created_at DESC);

-- Create schema_dependencies table
CREATE TABLE IF NOT EXISTS schema_dependencies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schema_id UUID NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    depends_on_schema_id UUID NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    dependency_type VARCHAR(50) NOT NULL CHECK (dependency_type IN ('REFERENCE', 'IMPORT', 'EXTENDS')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(schema_id, depends_on_schema_id, dependency_type)
);

CREATE INDEX idx_deps_schema_id ON schema_dependencies(schema_id);
CREATE INDEX idx_deps_depends_on ON schema_dependencies(depends_on_schema_id);

-- Create function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger for schemas table
CREATE TRIGGER update_schemas_updated_at
    BEFORE UPDATE ON schemas
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Insert default data
INSERT INTO schemas (namespace, name, version_major, version_minor, version_patch, format, content, content_hash, state, description)
VALUES
    ('system', 'health_check', 1, 0, 0, 'JSON',
     '{"type":"object","properties":{"status":{"type":"string"}}}',
     'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
     'ACTIVE',
     'Health check schema')
ON CONFLICT (namespace, name, version_major, version_minor, version_patch) DO NOTHING;
