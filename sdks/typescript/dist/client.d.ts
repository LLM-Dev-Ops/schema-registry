import { Schema, RegisterSchemaResponse, GetSchemaResponse, ValidateResponse, CompatibilityResult, SearchResult, ClientConfig } from './types';
/**
 * TypeScript client for the LLM Schema Registry
 */
export declare class SchemaRegistryClient {
    private client;
    private cache;
    constructor(config: ClientConfig);
    /**
     * Register a new schema
     */
    registerSchema(schema: Schema): Promise<RegisterSchemaResponse>;
    /**
     * Get a schema by namespace, name, and version
     */
    getSchema(namespace: string, name: string, version: string): Promise<GetSchemaResponse>;
    /**
     * Get the latest version of a schema
     */
    getLatestSchema(namespace: string, name: string): Promise<GetSchemaResponse>;
    /**
     * Validate data against a schema
     */
    validate(namespace: string, name: string, version: string, data: unknown): Promise<ValidateResponse>;
    /**
     * Check compatibility between schemas
     */
    checkCompatibility(namespace: string, name: string, newSchema: string): Promise<CompatibilityResult>;
    /**
     * Search for schemas
     */
    searchSchemas(query: string): Promise<SearchResult>;
    /**
     * List all schemas in a namespace
     */
    listSchemas(namespace: string): Promise<SearchResult>;
    /**
     * Delete a specific schema version
     */
    deleteSchema(namespace: string, name: string, version: string): Promise<void>;
    /**
     * Clear the local cache
     */
    clearCache(): void;
    /**
     * Handle API errors and convert to appropriate error types
     */
    private handleError;
}
//# sourceMappingURL=client.d.ts.map