package annalib

import (
	"os"
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

func TestDetachedProcessAttr(t *testing.T) {
	attr := detachedProcessAttr()
	if attr == nil {
		t.Fatal("expected non-nil SysProcAttr")
	}
	if !attr.Setsid {
		t.Error("expected Setsid to be true")
	}
}

func TestStartBinaryNotFound(t *testing.T) {
	// Pin PATH to empty dir so server binaries are guaranteed not found.
	oldPath := os.Getenv("PATH")
	os.Setenv("PATH", "/nonexistent_dir_for_test")
	defer os.Setenv("PATH", oldPath)

	started, err := Start("/nonexistent/config.yml")
	if err == nil {
		t.Fatal("expected error when binary not found")
	}
	if started != 0 {
		t.Errorf("expected 0 started, got %d", started)
	}
	// Verify it is a ProcessError.
	if _, ok := err.(*ProcessError); !ok {
		t.Errorf("expected *ProcessError, got %T: %v", err, err)
	}
}

func TestProcessListContents(t *testing.T) {
	expected := []string{"anna-monitor", "anna-route", "anna-kvs"}
	if len(processList) != len(expected) {
		t.Fatalf("expected %d processes, got %d", len(expected), len(processList))
	}
	for i, name := range expected {
		if processList[i] != name {
			t.Errorf("processList[%d] = %q, want %q", i, processList[i], name)
		}
	}
}
