import { computeFingerprint } from '../fingerprint';

describe('computeFingerprint', () => {
    it('is deterministic for same inputs', () => {
        const a = computeFingerprint(2, 'doc.storage.saved', 'IO_ERROR', 'doc123');
        const b = computeFingerprint(2, 'doc.storage.saved', 'IO_ERROR', 'doc123');
        expect(a).toBe(b);
    });

    it('ctx_primary_key prevents collision', () => {
        const a = computeFingerprint(2, 'doc.storage.saved', 'IO_ERROR', 'doc123');
        const b = computeFingerprint(2, 'doc.storage.saved', 'IO_ERROR', 'doc456');
        expect(a).not.toBe(b);
    });

    it('differs across service codes', () => {
        const a = computeFingerprint(1, 'auth.jwt.expired', 'TOKEN_EXPIRED', '');
        const b = computeFingerprint(2, 'auth.jwt.expired', 'TOKEN_EXPIRED', '');
        expect(a).not.toBe(b);
    });

    it('service code is raw byte, not ASCII digit', () => {
        // service code 1 (raw 0x01) must differ from ASCII '1' (0x31)
        const rawByte   = computeFingerprint(1,    'e', 'c', '');
        const asciChar  = computeFingerprint(0x31, 'e', 'c', '');
        expect(rawByte).not.toBe(asciChar);
    });

    it('returns a bigint', () => {
        const result = computeFingerprint(2, 'doc.storage.saved', 'IO_ERROR', '');
        expect(typeof result).toBe('bigint');
    });
});
