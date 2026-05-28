import { Err, Outcome } from './fields';
import { KEY_TO_ABBREV } from './dict';
import { PART_A_SCHEMA_VERSION, PARENT_SPAN_ABSENT } from './wire-contract';

export interface LogEntry {
    // Part A
    traceId:      string;
    spanId:       string;
    parentSpan:   string;
    seq:          number;
    serviceCode:  number;
    levelCode:    number;
    outcomeCode:  number;
    tsMs:         number;
    // Part B
    event:        string;
    message:      string;
    durationMs:   number | undefined;
    outcome:      Outcome | undefined;
    error:        Err | undefined;
    userId:       string;
    journeyStage: string;
    ctx:          Record<string, unknown>;
}

// EncodePartA produces the 55-byte fixed-position header line.
// Matches Go: fmt.Sprintf("A:%d|%-14s|%-7s|%-7s|%02x|%d|%d|%d|%013d\n", ...)
export function encodePartA(entry: LogEntry): string {
    const parentSpan = entry.parentSpan || PARENT_SPAN_ABSENT;
    return (
        `A:${PART_A_SCHEMA_VERSION}|` +
        `${entry.traceId.substring(0, 14).padEnd(14)}|` +
        `${entry.spanId.substring(0, 7).padEnd(7)}|` +
        `${parentSpan.substring(0, 7).padEnd(7)}|` +
        `${(entry.seq & 0xff).toString(16).padStart(2, '0')}|` +
        `${entry.serviceCode}|` +
        `${entry.levelCode}|` +
        `${entry.outcomeCode}|` +
        `${entry.tsMs.toString().padStart(13, '0')}\n`
    );
}

// EncodePartB produces the abbreviated JSON payload line.
export function encodePartB(entry: LogEntry): string {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const out: Record<string, any> = {};
    out['e'] = entry.event;
    out['m'] = entry.message;
    if (entry.durationMs !== undefined)  out['ms'] = entry.durationMs;
    if (entry.outcome !== undefined)     out['o']  = entry.outcome;

    if (entry.error) {
        out['er'] = {
            ek: entry.error.kind,
            ec: entry.error.code,
            em: entry.error.message,
            rt: entry.error.retryable,
        };
    }

    const ctxKeys = Object.keys(entry.ctx);
    if (ctxKeys.length > 0) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const ctx: Record<string, any> = {};
        for (const [k, v] of Object.entries(entry.ctx)) {
            ctx[KEY_TO_ABBREV[k] ?? k] = v;
        }
        out['c'] = ctx;
    }

    if (entry.userId)       out['ui'] = entry.userId;
    if (entry.journeyStage) out['js'] = entry.journeyStage;

    return JSON.stringify(out) + '\n';
}
