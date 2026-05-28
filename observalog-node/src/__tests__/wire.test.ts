import { encodePartA, encodePartB, LogEntry } from '../wire';
import { Err, Outcome } from '../fields';
import * as C from '../wire-contract';

function minimalEntry(): LogEntry {
    return {
        traceId:      'trc_7f2a1b9e4d',
        spanId:       'spn_004',
        parentSpan:   'spn_001',
        seq:          4,
        serviceCode:  C.SERVICE_DOC,
        levelCode:    1,
        outcomeCode:  1,
        tsMs:         1748268153812,
        event:        'doc.storage.saved',
        message:      'Document written to storage',
        durationMs:   undefined,
        outcome:      undefined,
        error:        undefined,
        userId:       '',
        journeyStage: '',
        ctx:          {},
    };
}

describe('Part A encoding', () => {
    it('starts with A:1|', () => {
        expect(encodePartA(minimalEntry())).toMatch(/^A:1\|/);
    });

    it('is exactly 55 content chars + A: prefix + newline = 58 total', () => {
        const line = encodePartA(minimalEntry());
        expect(line.length).toBe(58); // "A:" + 55 content + "\n"
    });

    it('places trace_id at content offset 2 (after A:)', () => {
        const line = encodePartA(minimalEntry());
        // Offsets are within the content after the "A:" prefix.
        const payload = line.slice(2); // skip "A:"
        const traceId = payload.slice(C.TRACE_ID_OFFSET, C.TRACE_ID_OFFSET + 14).trimEnd();
        expect(traceId).toBe('trc_7f2a1b9e4d');
    });

    it('encodes seq as 2-char lowercase hex', () => {
        const e = { ...minimalEntry(), seq: 255 };
        const line = encodePartA(e);
        const payload = line.slice(2); // skip "A:"
        const seqField = payload.slice(C.SEQ_OFFSET, C.SEQ_OFFSET + 2);
        expect(seqField).toBe('ff');
    });

    it('uses "-------" for absent parent_span', () => {
        const e = { ...minimalEntry(), parentSpan: '' };
        const line = encodePartA(e);
        const payload = line.slice(2); // skip "A:"
        const parent = payload.slice(C.PARENT_SPAN_OFFSET, C.PARENT_SPAN_OFFSET + 7);
        expect(parent).toBe('-------');
    });
});

describe('Part B encoding', () => {
    it('abbreviates event to "e"', () => {
        const line = encodePartB(minimalEntry());
        const parsed = JSON.parse(line.trim());
        expect(parsed).toHaveProperty('e');
        expect(parsed).not.toHaveProperty('event');
    });

    it('abbreviates message to "m"', () => {
        const parsed = JSON.parse(encodePartB(minimalEntry()).trim());
        expect(parsed).toHaveProperty('m', 'Document written to storage');
    });

    it('encodes error struct with abbreviated keys', () => {
        const e = {
            ...minimalEntry(),
            error: new Err('RateLimitExceeded', 'QUOTA', 'limit hit', true),
        };
        const parsed = JSON.parse(encodePartB(e).trim());
        expect(parsed.er).toMatchObject({ ek: 'RateLimitExceeded', ec: 'QUOTA', rt: true });
    });

    it('encodes outcome string and duration_ms', () => {
        const e = { ...minimalEntry(), outcome: Outcome.Failure, durationMs: 87 };
        const parsed = JSON.parse(encodePartB(e).trim());
        expect(parsed.o).toBe('failure');
        expect(parsed.ms).toBe(87);
    });
});
