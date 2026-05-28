// eslint-disable-next-line @typescript-eslint/no-require-imports
const XXHash = require('xxhashjs') as typeof import('xxhashjs');

// Compute the xxHash64 fingerprint matching Go's ComputeFingerprint and brain's compute_fingerprint.
// Input: serviceCode (raw byte) | event | errorCode | ctxPrimaryKey joined with '|'.
// serviceCode is a raw byte — NOT the ASCII digit character.
// Matches Go: string(uint8(serviceCode)) which yields the raw byte value.
export function computeFingerprint(
    serviceCode: number,
    event: string,
    errorCode: string,
    ctxPrimaryKey: string,
): bigint {
    const data = Buffer.concat([
        Buffer.from([serviceCode & 0xff]),
        Buffer.from('|'),
        Buffer.from(event, 'utf8'),
        Buffer.from('|'),
        Buffer.from(errorCode, 'utf8'),
        Buffer.from('|'),
        Buffer.from(ctxPrimaryKey, 'utf8'),
    ]);

    const result = XXHash.h64(data, 0);
    const hex = result.toString(16).padStart(16, '0');
    // Convert unsigned uint64 → signed int64 to match Go's int64 and PostgreSQL BIGINT.
    let n = BigInt('0x' + hex);
    if (n >= BigInt('0x8000000000000000')) n -= BigInt('0x10000000000000000');
    return n;
}
