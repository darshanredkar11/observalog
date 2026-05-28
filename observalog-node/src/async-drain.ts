// Non-blocking log buffer drained via setImmediate.
// Mirrors observalog-go/async.go: non-blocking send, drop counter, flush on shutdown.
const buffer: string[] = [];
let draining       = false;
let droppedCount   = 0;
let maxBufferSize  = 10_000;

export function configure(size: number): void {
    maxBufferSize = size;
}

export function send(line: string): void {
    if (buffer.length >= maxBufferSize) {
        droppedCount++;
        return;
    }
    buffer.push(line);
    if (!draining) {
        draining = true;
        setImmediate(drain);
    }
}

function drain(): void {
    while (buffer.length > 0) {
        // process.stdout.write is synchronous for small payloads on Node.js.
        process.stdout.write(buffer.shift()!);
    }
    draining = false;
}

export function flush(): Promise<void> {
    return new Promise<void>(resolve => {
        if (buffer.length === 0) { resolve(); return; }
        const check = (): void => {
            if (buffer.length === 0) { resolve(); return; }
            setImmediate(check);
        };
        setImmediate(check);
    });
}

export function droppedLogCount(): number {
    return droppedCount;
}
