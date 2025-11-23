/**
 * Type definitions for the LLM Schema Registry TypeScript SDK
 */
/** Supported schema formats */
export declare enum SchemaFormat {
    JSON_SCHEMA = "json_schema",
    AVRO = "avro",
    PROTOBUF = "protobuf"
}
/** Schema compatibility modes */
export declare enum CompatibilityMode {
    BACKWARD = "backward",
    FORWARD = "forward",
    FULL = "full",
    BACKWARD_TRANSITIVE = "backward_transitive",
    FORWARD_TRANSITIVE = "forward_transitive",
    FULL_TRANSITIVE = "full_transitive",
    NONE = "none"
}
/** Schema metadata */
export interface SchemaMetadata {
    description?: string;
    tags?: string[];
    owner?: string;
    custom?: Record<string, unknown>;
}
/** Schema definition */
export interface Schema {
    namespace: string;
    name: string;
    version: string;
    format: SchemaFormat;
    content: string;
    metadata?: SchemaMetadata;
}
/** Response from schema registration */
export interface RegisterSchemaResponse {
    schema_id: string;
    version: string;
    created_at: string;
}
/** Response from schema retrieval */
export interface GetSchemaResponse {
    schema_id: string;
    namespace: string;
    name: string;
    version: string;
    format: SchemaFormat;
    content: string;
    metadata?: SchemaMetadata;
    created_at: string;
    updated_at: string;
}
/** Response from data validation */
export interface ValidateResponse {
    is_valid: boolean;
    errors: string[];
}
/** Result of compatibility checking */
export interface CompatibilityResult {
    is_compatible: boolean;
    incompatibilities: string[];
    mode: CompatibilityMode;
}
/** Schema version information */
export interface SchemaVersion {
    version: string;
    schema_id: string;
    created_at: string;
}
/** Schema search result */
export interface SearchResult {
    schema_id: string;
    namespace: string;
    name: string;
    version: string;
    description?: string;
    tags?: string[];
    score: number;
}
/** Client configuration */
export interface ClientConfig {
    /** Base URL of the schema registry */
    baseURL: string;
    /** Optional API key for authentication */
    apiKey?: string;
    /** Request timeout in milliseconds (default: 30000) */
    timeout?: number;
    /** Maximum number of retry attempts (default: 3) */
    maxRetries?: number;
    /** Cache TTL in milliseconds (default: 300000) */
    cacheTTL?: number;
    /** Maximum number of cached items (default: 1000) */
    cacheMaxSize?: number;
}
//# sourceMappingURL=types.d.ts.map