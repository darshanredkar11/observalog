package log

import (
	"testing"
)

func TestFingerprintNoDuplicatesWithDifferentCtxKey(t *testing.T) {
	// Gap 3: Fingerprint collision prevention via ctx_primary_key.
	// Same event and error_code but different ctx_primary_key should produce
	// different fingerprints.

	event := "provider.send.rejected"
	errorCode := "PROVIDER_QUOTA_EXCEEDED"
	svc := uint8(ServiceProvider)

	fp1 := ComputeFingerprint(svc, event, errorCode, "doc_id_001")
	fp2 := ComputeFingerprint(svc, event, errorCode, "doc_id_002")

	if fp1 == fp2 {
		t.Errorf("fingerprints should differ for different ctx_primary_key: %d == %d", fp1, fp2)
	}
}

func TestFingerprintConsistent(t *testing.T) {
	// Same inputs should produce same fingerprint (deterministic).
	svc := uint8(ServiceAuth)
	event := "auth.jwt.validated"
	errorCode := "JWT_INVALID"
	ctxKey := "user_id_123"

	fp1 := ComputeFingerprint(svc, event, errorCode, ctxKey)
	fp2 := ComputeFingerprint(svc, event, errorCode, ctxKey)

	if fp1 != fp2 {
		t.Errorf("fingerprints should be consistent: %d != %d", fp1, fp2)
	}
}
