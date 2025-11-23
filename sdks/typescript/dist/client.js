"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.SchemaRegistryClient = void 0;
const axios_1 = __importDefault(require("axios"));
const axios_retry_1 = __importDefault(require("axios-retry"));
const lru_cache_1 = require("lru-cache");
const errors_1 = require("./errors");
/**
 * TypeScript client for the LLM Schema Registry
 */
class SchemaRegistryClient {
    constructor(config) {
        this.client = axios_1.default.create({
            baseURL: config.baseURL,
            timeout: config.timeout || 30000,
            headers: {
                'Content-Type': 'application/json',
                ...(config.apiKey && { 'X-API-Key': config.apiKey }),
            },
        });
        // Configure retry logic
        (0, axios_retry_1.default)(this.client, {
            retries: config.maxRetries || 3,
            retryDelay: axios_retry_1.default.exponentialDelay,
            retryCondition: (error) => {
                return (axios_retry_1.default.isNetworkOrIdempotentRequestError(error) ||
                    error.response?.status === 429 ||
                    (error.response?.status || 0) >= 500);
            },
        });
        // Initialize cache
        this.cache = new lru_cache_1.LRUCache({
            max: config.cacheMaxSize || 1000,
            ttl: config.cacheTTL || 300000, // Default: 5 minutes in milliseconds
        });
    }
    /**
     * Register a new schema
     */
    async registerSchema(schema) {
        try {
            const response = await this.client.post('/schemas', schema);
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Get a schema by namespace, name, and version
     */
    async getSchema(namespace, name, version) {
        const cacheKey = `${namespace}:${name}:${version}`;
        const cached = this.cache.get(cacheKey);
        if (cached) {
            return cached;
        }
        try {
            const response = await this.client.get(`/schemas/${namespace}/${name}/${version}`);
            this.cache.set(cacheKey, response.data);
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Get the latest version of a schema
     */
    async getLatestSchema(namespace, name) {
        try {
            const response = await this.client.get(`/schemas/${namespace}/${name}/latest`);
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Validate data against a schema
     */
    async validate(namespace, name, version, data) {
        try {
            const response = await this.client.post(`/schemas/${namespace}/${name}/${version}/validate`, { data });
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Check compatibility between schemas
     */
    async checkCompatibility(namespace, name, newSchema) {
        try {
            const response = await this.client.post(`/schemas/${namespace}/${name}/compatibility`, { schema: newSchema });
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Search for schemas
     */
    async searchSchemas(query) {
        try {
            const response = await this.client.get('/schemas/search', {
                params: { q: query },
            });
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * List all schemas in a namespace
     */
    async listSchemas(namespace) {
        try {
            const response = await this.client.get(`/schemas/${namespace}`);
            return response.data;
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Delete a specific schema version
     */
    async deleteSchema(namespace, name, version) {
        try {
            await this.client.delete(`/schemas/${namespace}/${name}/${version}`);
            // Invalidate cache
            const cacheKey = `${namespace}:${name}:${version}`;
            this.cache.delete(cacheKey);
        }
        catch (error) {
            throw this.handleError(error);
        }
    }
    /**
     * Clear the local cache
     */
    clearCache() {
        this.cache.clear();
    }
    /**
     * Handle API errors and convert to appropriate error types
     */
    handleError(error) {
        if (axios_1.default.isAxiosError(error)) {
            const axiosError = error;
            const status = axiosError.response?.status;
            const message = axiosError.response?.data?.message || axiosError.response?.data?.error || axiosError.message;
            const errors = axiosError.response?.data?.errors || [message];
            switch (status) {
                case 401:
                case 403:
                    return new errors_1.AuthenticationError(message);
                case 404:
                    return new errors_1.SchemaNotFoundError(message);
                case 409:
                    return new errors_1.IncompatibleSchemaError(errors);
                case 422:
                    return new errors_1.SchemaValidationError(errors);
                case 429:
                    return new errors_1.RateLimitError();
                default:
                    return new errors_1.SchemaRegistryError(message, status);
            }
        }
        if (error instanceof Error) {
            return new errors_1.SchemaRegistryError(error.message);
        }
        return new errors_1.SchemaRegistryError('Unknown error occurred');
    }
}
exports.SchemaRegistryClient = SchemaRegistryClient;
//# sourceMappingURL=client.js.map