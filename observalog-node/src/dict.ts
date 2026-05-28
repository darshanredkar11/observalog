// Abbreviation dictionary — must mirror observalog-go/dict.go and brain/parser.rs.
export const DICT_VERSION = 1;

export const KEY_TO_ABBREV: Record<string, string> = {
    event:           'e',
    message:         'm',
    duration_ms:     'ms',
    ctx:             'c',
    outcome:         'o',
    error:           'er',
    'error.kind':    'ek',
    'error.code':    'ec',
    'error.message': 'em',
    'error.retryable': 'rt',
    doc_id:          'di',
    topic:           'tp',
    partition:       'pt',
    offset:          'of',
    provider:        'pr',
    http_status:     'hs',
    user_id:         'ui',
    journey_stage:   'js',
};

export const ABBREV_TO_KEY: Record<string, string> = Object.fromEntries(
    Object.entries(KEY_TO_ABBREV).map(([k, v]) => [v, k]),
);

export function validateNoDictCollisions(): void {
    const seen = new Map<string, string>();
    for (const [key, abbrev] of Object.entries(KEY_TO_ABBREV)) {
        if (seen.has(abbrev)) {
            throw new Error(
                `ObservaLog dict collision: abbreviation "${abbrev}" used by both ` +
                `"${seen.get(abbrev)}" and "${key}"`,
            );
        }
        seen.set(abbrev, key);
    }
}
