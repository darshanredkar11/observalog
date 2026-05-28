package com.observalog;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

// Abbreviation dictionary — must mirror observalog-go/dict.go and brain/parser.rs.
// Gap 8: versioned dict prevents collision across rolling deploys.
public final class Dict {
    private Dict() {}

    public static final int DICT_VERSION = 1;

    public static final Map<String, String> KEY_TO_ABBREV;
    public static final Map<String, String> ABBREV_TO_KEY;

    static {
        Map<String, String> m = new HashMap<>();
        m.put("event",           "e");
        m.put("message",         "m");
        m.put("duration_ms",     "ms");
        m.put("ctx",             "c");
        m.put("outcome",         "o");
        m.put("error",           "er");
        m.put("error.kind",      "ek");
        m.put("error.code",      "ec");
        m.put("error.message",   "em");
        m.put("error.retryable", "rt");
        m.put("doc_id",          "di");
        m.put("topic",           "tp");
        m.put("partition",       "pt");
        m.put("offset",          "of");
        m.put("provider",        "pr");
        m.put("http_status",     "hs");
        m.put("user_id",         "ui");
        m.put("journey_stage",   "js");
        KEY_TO_ABBREV = Collections.unmodifiableMap(m);

        Map<String, String> r = new HashMap<>();
        for (Map.Entry<String, String> e : m.entrySet()) {
            r.put(e.getValue(), e.getKey());
        }
        ABBREV_TO_KEY = Collections.unmodifiableMap(r);

        validateNoDictCollisions();
    }

    private static void validateNoDictCollisions() {
        Map<String, String> seen = new HashMap<>();
        for (Map.Entry<String, String> e : KEY_TO_ABBREV.entrySet()) {
            if (seen.containsKey(e.getValue())) {
                throw new IllegalStateException(
                    "ObservaLog dict collision: abbreviation \"" + e.getValue() +
                    "\" used by both \"" + seen.get(e.getValue()) + "\" and \"" + e.getKey() + "\""
                );
            }
            seen.put(e.getValue(), e.getKey());
        }
    }
}
