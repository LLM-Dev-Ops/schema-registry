"use strict";
/**
 * Error classes for the LLM Schema Registry TypeScript SDK
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.ServerError = exports.RateLimitError = exports.AuthorizationError = exports.AuthenticationError = exports.IncompatibleSchemaError = exports.SchemaValidationError = exports.SchemaNotFoundError = exports.SchemaRegistryError = void 0;
/** Base error for all schema registry errors */
class SchemaRegistryError extends Error {
    constructor(message, statusCode) {
        super(message);
        this.statusCode = statusCode;
        this.name = 'SchemaRegistryError';
        Object.setPrototypeOf(this, SchemaRegistryError.prototype);
    }
}
exports.SchemaRegistryError = SchemaRegistryError;
/** Error thrown when a schema is not found */
class SchemaNotFoundError extends SchemaRegistryError {
    constructor(schemaId) {
        super(`Schema not found: ${schemaId}`, 404);
        this.schemaId = schemaId;
        this.name = 'SchemaNotFoundError';
        Object.setPrototypeOf(this, SchemaNotFoundError.prototype);
    }
}
exports.SchemaNotFoundError = SchemaNotFoundError;
/** Error thrown when schema validation fails */
class SchemaValidationError extends SchemaRegistryError {
    constructor(errors) {
        const message = 'Schema validation failed:\n' + errors.map((e) => `  - ${e}`).join('\n');
        super(message, 400);
        this.errors = errors;
        this.name = 'SchemaValidationError';
        Object.setPrototypeOf(this, SchemaValidationError.prototype);
    }
}
exports.SchemaValidationError = SchemaValidationError;
/** Error thrown when schemas are incompatible */
class IncompatibleSchemaError extends SchemaRegistryError {
    constructor(incompatibilities) {
        const message = 'Schema compatibility check failed:\n' + incompatibilities.map((i) => `  - ${i}`).join('\n');
        super(message, 409);
        this.incompatibilities = incompatibilities;
        this.name = 'IncompatibleSchemaError';
        Object.setPrototypeOf(this, IncompatibleSchemaError.prototype);
    }
}
exports.IncompatibleSchemaError = IncompatibleSchemaError;
/** Error thrown when authentication fails */
class AuthenticationError extends SchemaRegistryError {
    constructor(message = 'Authentication failed') {
        super(message, 401);
        this.name = 'AuthenticationError';
        Object.setPrototypeOf(this, AuthenticationError.prototype);
    }
}
exports.AuthenticationError = AuthenticationError;
/** Error thrown when authorization fails */
class AuthorizationError extends SchemaRegistryError {
    constructor(message = 'Insufficient permissions') {
        super(message, 403);
        this.name = 'AuthorizationError';
        Object.setPrototypeOf(this, AuthorizationError.prototype);
    }
}
exports.AuthorizationError = AuthorizationError;
/** Error thrown when rate limit is exceeded */
class RateLimitError extends SchemaRegistryError {
    constructor(retryAfter) {
        const message = retryAfter
            ? `Rate limit exceeded. Retry after ${retryAfter} seconds`
            : 'Rate limit exceeded';
        super(message, 429);
        this.retryAfter = retryAfter;
        this.name = 'RateLimitError';
        Object.setPrototypeOf(this, RateLimitError.prototype);
    }
}
exports.RateLimitError = RateLimitError;
/** Error thrown when server encounters an error */
class ServerError extends SchemaRegistryError {
    constructor(message = 'Internal server error') {
        super(message, 500);
        this.name = 'ServerError';
        Object.setPrototypeOf(this, ServerError.prototype);
    }
}
exports.ServerError = ServerError;
//# sourceMappingURL=errors.js.map