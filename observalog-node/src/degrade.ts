import { v4 as uuidv4 } from 'uuid';
import { LogContextData } from './context';

// Decision 5: never throw on missing context. Background jobs are legitimate.
// The sys_ prefix signals the brain without any special emitter code.
export function applyGracefulDegradation(ctx: LogContextData): void {
    if (!ctx.traceId) {
        const uid = uuidv4().replace(/-/g, '');
        ctx.traceId = 'sys_' + uid.substring(0, 10); // 14 chars total — fits CHAR(14)
    }
    if (!ctx.userId) {
        ctx.userId = 'system';
    }
    if (!ctx.journeyStage) {
        ctx.journeyStage = 'system.background.untraced';
    }
}
