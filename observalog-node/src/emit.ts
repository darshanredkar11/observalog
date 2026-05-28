import { F, Err, Outcome, Level } from './fields';
import { getContext, LogContextData } from './context';
import { applyGracefulDegradation } from './degrade';
import { encodePartA, encodePartB, LogEntry } from './wire';
import { send } from './async-drain';
import { SERVICE_SYSTEM, SERVICE_AUTH, SERVICE_DOC, SERVICE_PROVIDER } from './wire-contract';

let minLevel    = Level.Info;
let serviceCode = SERVICE_SYSTEM;

export function setMinLevel(l: Level): void    { minLevel = l; }
export function setServiceCode(c: number): void { serviceCode = c; }
export function getServiceCode(): number        { return serviceCode; }

export function debug(event: string, message: string, fields: F = {}): void {
    emitLine(Level.Debug, event, message, fields);
}

export function info(event: string, message: string, fields: F = {}): void {
    emitLine(Level.Info, event, message, fields);
}

export function warn(event: string, message: string, fields: F = {}): void {
    emitLine(Level.Warn, event, message, fields);
}

export function error(event: string, message: string, fields: F = {}): void {
    emitLine(Level.Error, event, message, fields);
}

function emitLine(level: Level, event: string, message: string, fields: F): void {
    if (level < minLevel) return;

    // Get or create a degraded context
    const stored = getContext();
    const ctx: LogContextData = stored ?? {
        traceId: '', spanId: '', parentSpan: '',
        userId: '', journeyStage: '', seq: { value: 0 },
    };
    applyGracefulDegradation(ctx);

    // Extract structural fields from F
    let outcome:    Outcome | undefined;
    let durationMs: number  | undefined;
    let errField:   Err     | undefined;
    const devCtx: Record<string, unknown> = {};

    for (const [k, v] of Object.entries(fields)) {
        if (k === 'outcome' && typeof v === 'string') {
            outcome = v as Outcome;
        } else if (k === 'duration_ms' && typeof v === 'number') {
            durationMs = v;
        } else if (k === 'error' && v instanceof Err) {
            errField = v;
        } else {
            devCtx[k] = v;
        }
    }

    const outcomeCode = outcomeToCode(outcome);
    const seqVal = ++ctx.seq.value;

    const entry: LogEntry = {
        traceId:      ctx.traceId,
        spanId:       ctx.spanId,
        parentSpan:   ctx.parentSpan,
        seq:          seqVal,
        serviceCode,
        levelCode:    level,
        outcomeCode,
        tsMs:         Date.now(),
        event,
        message,
        durationMs,
        outcome,
        error:        errField,
        userId:       ctx.userId,
        journeyStage: ctx.journeyStage,
        ctx:          devCtx,
    };

    send(encodePartA(entry));
    send(encodePartB(entry));
}

function outcomeToCode(o: Outcome | undefined): number {
    switch (o) {
        case Outcome.Success: return 1;
        case Outcome.Failure: return 2;
        case Outcome.Partial: return 3;
        case Outcome.Pending: return 4;
        default:              return 0;
    }
}

export function resolveServiceCode(name: string): number {
    switch (name) {
        case 'auth':     return SERVICE_AUTH;
        case 'doc':      return SERVICE_DOC;
        case 'provider': return SERVICE_PROVIDER;
        default:         return SERVICE_SYSTEM;
    }
}
