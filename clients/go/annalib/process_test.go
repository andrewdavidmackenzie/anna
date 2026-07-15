package annalib

import (
	"testing"
)

func TestStatusNothingRunning(t *testing.T) {
	statuses := Status()
	if len(statuses) != 3 {
		t.Fatalf("expected 3 process statuses, got %d", len(statuses))
	}
	for _, s := range statuses {
		if len(s.PIDs) != 0 {
			t.Errorf("expected no PIDs for %s, got %v", s.Name, s.PIDs)
		}
	}
}

func TestStatusProcessNames(t *testing.T) {
	statuses := Status()
	names := make(map[string]bool)
	for _, s := range statuses {
		names[s.Name] = true
	}
	for _, name := range []string{"anna-monitor", "anna-route", "anna-kvs"} {
		if !names[name] {
			t.Errorf("missing process name: %s", name)
		}
	}
}

func TestStopNothingRunning(t *testing.T) {
	count, err := Stop()
	if err != nil {
		t.Fatalf("Stop failed: %v", err)
	}
	if count != 0 {
		t.Errorf("expected 0 processes stopped, got %d", count)
	}
}

func TestPidsFromNameNonexistent(t *testing.T) {
	pids := pidsFromName("nonexistent_process_xyz_12345")
	if len(pids) != 0 {
		t.Errorf("expected no PIDs, got %v", pids)
	}
}
