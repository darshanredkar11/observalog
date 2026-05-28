import { v4 as uuidv4 } from 'uuid';
import { runWithContextAsync } from '../context';

type Request  = { headers: Record<string, string | string[] | undefined>; path: string };
type Response = Record<string, unknown>;
type NextFn   = () => Promise<void> | void;

// Express-compatible middleware that injects trace context into AsyncLocalStorage.
export function observalogMiddleware(
    req: Request,
    _res: Response,
    next: NextFn,
): void | Promise<void> {
    const h = (name: string): string => {
        const v = req.headers[name.toLowerCase()];
        return Array.isArray(v) ? v[0] : (v ?? '');
    };

    // Continue an existing trace OR start a fresh one at the entry point.
    // Never leave traceId empty — applyGracefulDegradation generates a NEW
    // sys_<uuid> for every emitLine call, so all logs in one request would
    // have different trace IDs and the brain could not correlate them.
    const incomingTraceId = h('x-trace-id');
    const traceId = incomingTraceId !== ''
        ? incomingTraceId
        : 'trc_' + uuidv4().replace(/-/g, '').substring(0, 10); // 14 chars: trc_ + 10

    // Prefer inherited journey_stage; fall back to auto-derive.
    const incomingStage = h('x-journey-stage');

    const ctx = {
        traceId,
        spanId:       'spn_' + uuidv4().substring(0, 3),
        parentSpan:   h('x-parent-span-id'),  // upstream span → our parent_span
        userId:       h('x-user-id'),
        journeyStage: incomingStage !== '' ? incomingStage : deriveJourneyStage(req.path),
        seq:          { value: 0 },
    };

    const result = next();
    if (result instanceof Promise) {
        return runWithContextAsync(ctx, () => result);
    }
    runWithContextAsync(ctx, async () => {}); // sync path — seed context then return
    return result;
}

// Auto-derives journey_stage from request path.
// /api/v1/docs/123/edit → http.docs.edit (strips /api, /v1, drops numeric/UUID segments)
function deriveJourneyStage(path: string): string {
    let p = path;
    for (const strip of ['/api', '/v1']) {
        if (p.startsWith(strip)) p = p.slice(strip.length);
    }
    const parts = p.split('/').filter(s =>
        s.length > 0 && !/^([0-9a-f]{8,}|[0-9]+)$/i.test(s),
    );
    return parts.length > 0 ? `http.${parts.join('.')}` : 'http.root';
}
