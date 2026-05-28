export { F, Err, Outcome, Level } from './fields';
export { Config, configFromEnv, init, shutdown } from './config';
export { debug, info, warn, error } from './emit';
export { getContext, runWithContext, runWithContextAsync } from './context';
export { computeFingerprint } from './fingerprint';

// Re-export drain for advanced use
export { droppedLogCount as droppedCount } from './async-drain';

// Outgoing HTTP client helpers (trace propagation to downstream services)
export {
    injectOutgoingHeaders,
    tracedFetch,
    attachAxiosTracing,
} from './middleware/http-client';
