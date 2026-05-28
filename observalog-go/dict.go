package log

// DictVersion must be incremented whenever KeyToAbbrev changes.
// Gap 8: versioned dict prevents abbreviation collisions across deploys.
const DictVersion = 1

// KeyToAbbrev maps full JSON field names to their wire abbreviations.
// Every key here must have a unique abbreviation — ValidateNoDictCollisions enforces this.
var KeyToAbbrev = map[string]string{
	"event":           "e",
	"message":         "m",
	"duration_ms":     "ms",
	"ctx":             "c",
	"outcome":         "o",
	"error":           "er",
	"error.kind":      "ek",
	"error.code":      "ec",
	"error.message":   "em",
	"error.retryable": "rt",
	"doc_id":          "di",
	"topic":           "tp",
	"partition":       "pt",
	"offset":          "of",
	"provider":        "pr",
	"http_status":     "hs",
	"user_id":         "ui",
	"journey_stage":   "js",
}

// AbbrevToKey is the reverse lookup used by observalog-brain to decode Part B.
var AbbrevToKey map[string]string

func init() {
	AbbrevToKey = make(map[string]string, len(KeyToAbbrev))
	for k, v := range KeyToAbbrev {
		AbbrevToKey[v] = k
	}
}

// ValidateNoDictCollisions panics if any two keys share an abbreviation.
// Called by Init() at startup — fail fast, never emit corrupt logs.
func ValidateNoDictCollisions() {
	seen := make(map[string]string, len(KeyToAbbrev))
	for key, abbrev := range KeyToAbbrev {
		if prior, exists := seen[abbrev]; exists {
			panic("observalog dict collision: abbreviation \"" + abbrev +
				"\" used by both \"" + prior + "\" and \"" + key + "\"")
		}
		seen[abbrev] = key
	}
}
