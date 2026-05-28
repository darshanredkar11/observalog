import { applyGracefulDegradation } from '../degrade';
import { LogContextData } from '../context';

function emptyCtx(): LogContextData {
    return { traceId: '', spanId: '', parentSpan: '', userId: '', journeyStage: '', seq: { value: 0 } };
}

describe('applyGracefulDegradation', () => {
    it('gives sys_ prefix to missing trace_id', () => {
        const ctx = emptyCtx();
        applyGracefulDegradation(ctx);
        expect(ctx.traceId).toMatch(/^sys_/);
        expect(ctx.traceId.length).toBe(14);
    });

    it('defaults userId to "system"', () => {
        const ctx = emptyCtx();
        applyGracefulDegradation(ctx);
        expect(ctx.userId).toBe('system');
    });

    it('defaults journeyStage to "system.background.untraced"', () => {
        const ctx = emptyCtx();
        applyGracefulDegradation(ctx);
        expect(ctx.journeyStage).toBe('system.background.untraced');
    });

    it('does not overwrite existing values', () => {
        const ctx: LogContextData = {
            traceId: 'trc_existing1234', spanId: '', parentSpan: '',
            userId: 'user-99', journeyStage: 'http.doc.upload', seq: { value: 0 },
        };
        applyGracefulDegradation(ctx);
        expect(ctx.traceId).toBe('trc_existing1234');
        expect(ctx.userId).toBe('user-99');
        expect(ctx.journeyStage).toBe('http.doc.upload');
    });
});
