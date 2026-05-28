import { AsyncLocalStorage } from 'async_hooks';

// Request-scoped context stored in AsyncLocalStorage.
// Node.js equivalent of Go's context.Context carrying observalog keys.
// Set by HTTP or Kafka middleware; automatically scoped to the async call tree.
export interface LogContextData {
    traceId:      string;
    spanId:       string;
    parentSpan:   string;
    userId:       string;
    journeyStage: string;
    // Mutable reference so seq increments are visible across the call tree.
    seq: { value: number };
}

const storage = new AsyncLocalStorage<LogContextData>();

export function getContext(): LogContextData | undefined {
    return storage.getStore();
}

// Run fn within a new context that inherits current values and overrides with provided fields.
export function runWithContext(overrides: Partial<LogContextData>, fn: () => void): void {
    const current = storage.getStore();
    const ctx: LogContextData = {
        traceId:      '',
        spanId:       '',
        parentSpan:   '',
        userId:       '',
        journeyStage: '',
        seq:          { value: 0 },
        ...current,
        ...overrides,
    };
    storage.run(ctx, fn);
}

// Async variant for middleware that calls next() which returns a Promise.
export function runWithContextAsync(
    overrides: Partial<LogContextData>,
    fn: () => Promise<void>,
): Promise<void> {
    const current = storage.getStore();
    const ctx: LogContextData = {
        traceId:      '',
        spanId:       '',
        parentSpan:   '',
        userId:       '',
        journeyStage: '',
        seq:          { value: 0 },
        ...current,
        ...overrides,
    };
    return storage.run(ctx, fn);
}
