/**
 * Outgoing HTTP client helpers for ObservaLog trace propagation.
 *
 * When Service A makes an HTTP call to Service B, Service B's middleware
 * needs to receive the trace context in request headers — otherwise it
 * starts a fresh trace and the brain cannot reconstruct the full journey.
 *
 * Three integration points are provided:
 *   1. injectOutgoingHeaders — low-level, works with any HTTP library
 *   2. tracedFetch           — drop-in replacement for globalThis.fetch
 *   3. attachAxiosTracing    — add to any existing Axios instance
 */

import { getContext } from '../context';

// ─── 1. Low-level header injection ──────────────────────────────────────────

/**
 * Reads the current trace context from AsyncLocalStorage and writes it into
 * the provided headers object.  Call this before every outgoing HTTP request
 * when using a raw HTTP client that doesn't support interceptors.
 *
 * @param headers  A mutable record of header name → value (lowercase names).
 *
 * @example
 * const headers: Record<string, string> = {};
 * injectOutgoingHeaders(headers);
 * await got(url, { headers });
 */
export function injectOutgoingHeaders(headers: Record<string, string>): void {
    const ctx = getContext();
    if (!ctx) return;

    if (ctx.traceId)      headers['x-trace-id']        = ctx.traceId;
    // Our span becomes the downstream parent_span — links the span tree.
    if (ctx.spanId)       headers['x-parent-span-id']  = ctx.spanId;
    if (ctx.userId)       headers['x-user-id']         = ctx.userId;
    if (ctx.journeyStage) headers['x-journey-stage']   = ctx.journeyStage;
}

// ─── 2. Traced fetch wrapper ─────────────────────────────────────────────────

type FetchInput  = Parameters<typeof fetch>[0];
type FetchInit   = Parameters<typeof fetch>[1];

/**
 * A drop-in replacement for `fetch` that automatically injects ObservaLog
 * trace context headers into every request.
 *
 * @example
 * // Replace all fetch calls in Service A with tracedFetch
 * import { tracedFetch as fetch } from '@darshanredkar11/observalog-node/middleware';
 *
 * const resp = await fetch('http://service-b/api/docs', { method: 'GET' });
 */
export async function tracedFetch(input: FetchInput, init?: FetchInit): Promise<Response> {
    const ctx = getContext();
    if (!ctx) {
        return fetch(input, init);
    }

    // Build a new Headers object so we don't mutate the caller's init.
    const headers = new Headers(init?.headers);
    if (ctx.traceId)      headers.set('x-trace-id',       ctx.traceId);
    if (ctx.spanId)       headers.set('x-parent-span-id', ctx.spanId);
    if (ctx.userId)       headers.set('x-user-id',        ctx.userId);
    if (ctx.journeyStage) headers.set('x-journey-stage',  ctx.journeyStage);

    return fetch(input, { ...init, headers });
}

// ─── 3. Axios interceptor ────────────────────────────────────────────────────

/**
 * Shape of an Axios-like instance — typed loosely so the library does not
 * need axios as a production dependency.
 */
interface AxiosLike {
    interceptors: {
        request: {
            use(onFulfilled: (config: AxiosRequestConfig) => AxiosRequestConfig): void;
        };
    };
}

interface AxiosRequestConfig {
    headers?: Record<string, string>;
    [key: string]: unknown;
}

/**
 * Attaches an ObservaLog tracing interceptor to an Axios instance.
 * Call once during service startup, after creating the Axios instance.
 *
 * @example
 * import axios from 'axios';
 * import { attachAxiosTracing } from '@darshanredkar11/observalog-node/middleware';
 *
 * const client = axios.create({ baseURL: 'http://service-b' });
 * attachAxiosTracing(client);
 * // All requests made with `client` now carry trace headers automatically.
 */
export function attachAxiosTracing(axiosInstance: AxiosLike): void {
    axiosInstance.interceptors.request.use((config: AxiosRequestConfig) => {
        const ctx = getContext();
        if (!ctx) return config;

        config.headers = config.headers ?? {};
        if (ctx.traceId)      config.headers['x-trace-id']        = ctx.traceId;
        if (ctx.spanId)       config.headers['x-parent-span-id']  = ctx.spanId;
        if (ctx.userId)       config.headers['x-user-id']         = ctx.userId;
        if (ctx.journeyStage) config.headers['x-journey-stage']   = ctx.journeyStage;
        return config;
    });
}
