package wardnet_test

import (
	"encoding/json"
	"testing"

	wardnet "wardnet.network/go"
)

func TestInstallPhaseRoundTrip(t *testing.T) {
	const wire = `{"phase":"downloading","bytes":5,"total":10}`
	var p wardnet.InstallPhase
	if err := json.Unmarshal([]byte(wire), &p); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if p.Kind != wardnet.InstallDownloading || p.Bytes != 5 || p.Total == nil || *p.Total != 10 {
		t.Fatalf("decoded phase = %+v", p)
	}
	got, err := json.Marshal(p)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if string(got) != wire {
		t.Errorf("marshal = %s, want %s", got, wire)
	}
}

func TestInstallPhaseFailedMarshal(t *testing.T) {
	p := wardnet.InstallPhase{Kind: wardnet.InstallFailed, Reason: "boom"}
	got, err := json.Marshal(p)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if string(got) != `{"phase":"failed","reason":"boom"}` {
		t.Errorf("marshal = %s", got)
	}
}

func TestBackupStatusRoundTrip(t *testing.T) {
	const wire = `{"state":"importing","phase":"extracting"}`
	var s wardnet.BackupStatus
	if err := json.Unmarshal([]byte(wire), &s); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if s.State != wardnet.BackupImporting || s.Phase != "extracting" {
		t.Fatalf("decoded status = %+v", s)
	}
	got, err := json.Marshal(s)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if string(got) != wire {
		t.Errorf("marshal = %s, want %s", got, wire)
	}
}
