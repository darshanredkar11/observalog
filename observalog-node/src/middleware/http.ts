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

    const ctx = {
        traceId:      h('x-trace-id'),
        spanId:       'spn_' + uuidv4().substring(0, 3),
        parentSpan:   h('x-parent-span-id'),
        userId:       h('x-user-id'),
        journeyStage: deriveJourneyStage(req.path),
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
