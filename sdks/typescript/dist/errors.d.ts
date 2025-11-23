/**
 * Error classes for the LLM Schema Registry TypeScript SDK
 */
/** Base error for all schema registry errors */
export declare class SchemaRegistryError extends Error {
    readonly statusCode?: number | undefined;
    constructor(message: string, statusCode?: number | undefined);
}
/** Error thrown when a schema is not found */
export declare class SchemaNotFoundError extends SchemaRegistryError {
    readonly schemaId: string;
    constructor(schemaId: string);
}
/** Error thrown when schema validation fails */
export declare class SchemaValidationError extends SchemaRegistryError {
    readonly errors: string[];
    constructor(errors: string[]);
}
/** Error thrown when schemas are incompatible */
export declare class IncompatibleSchemaError extends SchemaRegistryError {
    readonly incompatibilities: string[];
    constructor(incompatibilities: string[]);
}
/** Error thrown when authentication fails */
export declare class AuthenticationError extends SchemaRegistryError {
    constructor(message?: string);
}
/** Error thrown when authorization fails */
export declare class AuthorizationError extends SchemaRegistryError {
    constructor(message?: string);
}
/** Error thrown when rate limit is exceeded */
export declare class RateLimitError extends SchemaRegistryError {
    readonly retryAfter?: number | undefined;
    constructor(retryAfter?: number | undefined);
}
/** Error thrown when server encounters an error */
export declare class ServerError extends SchemaRegistryError {
    constructor(message?: string);
}
//# sourceMappingURL=errors.d.ts.map