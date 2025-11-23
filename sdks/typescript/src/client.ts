import axios, { AxiosInstance, AxiosError } from 'axios';
import axiosRetry from 'axios-retry';
import { LRUCache } from 'lru-cache';
import {
  Schema,
  RegisterSchemaResponse,
  GetSchemaResponse,
  ValidateResponse,
  CompatibilityResult,
  SearchResult,
  ClientConfig,
} from './types';
import {
  SchemaRegistryError,
  SchemaNotFoundError,
  SchemaValidationError,
  IncompatibleSchemaError,
  AuthenticationError,
  RateLimitError,
} from './errors';

/**
 * TypeScript client for the LLM Schema Registry
 */
export class SchemaRegistryClient {
  private client: AxiosInstance;
  private cache: LRUCache<string, GetSchemaResponse>;

  constructor(config: ClientConfig) {
    this.client = axios.create({
      baseURL: config.baseURL,
      timeout: config.timeout || 30000,
      headers: {
        'Content-Type': 'application/json',
        ...(config.apiKey && { 'X-API-Key': config.apiKey }),
      },
    });

    // Configure retry logic
    axiosRetry(this.client, {
      retries: config.maxRetries || 3,
      retryDelay: axiosRetry.exponentialDelay,
      retryCondition: (error) => {
        return (
          axiosRetry.isNetworkOrIdempotentRequestError(error) ||
          error.response?.status === 429 ||
          (error.response?.status || 0) >= 500
        );
      },
    });

    // Initialize cache
    this.cache = new LRUCache<string, GetSchemaResponse>({
      max: config.cacheMaxSize || 1000,
      ttl: config.cacheTTL || 300000, // Default: 5 minutes in milliseconds
    });
  }

  /**
   * Register a new schema
   */
  async registerSchema(schema: Schema): Promise<RegisterSchemaResponse> {
    try {
      const response = await this.client.post<RegisterSchemaResponse>(
        '/schemas',
        schema
      );
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Get a schema by namespace, name, and version
   */
  async getSchema(
    namespace: string,
    name: string,
    version: string
  ): Promise<GetSchemaResponse> {
    const cacheKey = `${namespace}:${name}:${version}`;
    const cached = this.cache.get(cacheKey);
    if (cached) {
      return cached;
    }

    try {
      const response = await this.client.get<GetSchemaResponse>(
        `/schemas/${namespace}/${name}/${version}`
      );
      this.cache.set(cacheKey, response.data);
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Get the latest version of a schema
   */
  async getLatestSchema(
    namespace: string,
    name: string
  ): Promise<GetSchemaResponse> {
    try {
      const response = await this.client.get<GetSchemaResponse>(
        `/schemas/${namespace}/${name}/latest`
      );
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Validate data against a schema
   */
  async validate(
    namespace: string,
    name: string,
    version: string,
    data: unknown
  ): Promise<ValidateResponse> {
    try {
      const response = await this.client.post<ValidateResponse>(
        `/schemas/${namespace}/${name}/${version}/validate`,
        { data }
      );
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Check compatibility between schemas
   */
  async checkCompatibility(
    namespace: string,
    name: string,
    newSchema: string
  ): Promise<CompatibilityResult> {
    try {
      const response = await this.client.post<CompatibilityResult>(
        `/schemas/${namespace}/${name}/compatibility`,
        { schema: newSchema }
      );
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Search for schemas
   */
  async searchSchemas(query: string): Promise<SearchResult> {
    try {
      const response = await this.client.get<SearchResult>('/schemas/search', {
        params: { q: query },
      });
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * List all schemas in a namespace
   */
  async listSchemas(namespace: string): Promise<SearchResult> {
    try {
      const response = await this.client.get<SearchResult>(
        `/schemas/${namespace}`
      );
      return response.data;
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Delete a specific schema version
   */
  async deleteSchema(
    namespace: string,
    name: string,
    version: string
  ): Promise<void> {
    try {
      await this.client.delete(`/schemas/${namespace}/${name}/${version}`);
      // Invalidate cache
      const cacheKey = `${namespace}:${name}:${version}`;
      this.cache.delete(cacheKey);
    } catch (error) {
      throw this.handleError(error);
    }
  }

  /**
   * Clear the local cache
   */
  clearCache(): void {
    this.cache.clear();
  }

  /**
   * Handle API errors and convert to appropriate error types
   */
  private handleError(error: unknown): Error {
    if (axios.isAxiosError(error)) {
      const axiosError = error as AxiosError<{ error?: string; message?: string; errors?: string[] }>;
      const status = axiosError.response?.status;
      const message = axiosError.response?.data?.message || axiosError.response?.data?.error || axiosError.message;
      const errors = axiosError.response?.data?.errors || [message];

      switch (status) {
        case 401:
        case 403:
          return new AuthenticationError(message);
        case 404:
          return new SchemaNotFoundError(message);
        case 409:
          return new IncompatibleSchemaError(errors);
        case 422:
          return new SchemaValidationError(errors);
        case 429:
          return new RateLimitError();
        default:
          return new SchemaRegistryError(message, status);
      }
    }

    if (error instanceof Error) {
      return new SchemaRegistryError(error.message);
    }

    return new SchemaRegistryError('Unknown error occurred');
  }
}
