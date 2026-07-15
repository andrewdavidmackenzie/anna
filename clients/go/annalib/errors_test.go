package annalib

import (
	"testing"
)

func TestConfigFileError(t *testing.T) {
	err := &ConfigFileError{Path: "/tmp/missing.yml", Detail: "file not found"}
	got := err.Error()
	if got != "could not load config from '/tmp/missing.yml': file not found" {
		t.Errorf("unexpected error string: %s", got)
	}
}

func TestKVSError(t *testing.T) {
	err := &KVSError{Message: "timeout"}
	got := err.Error()
	if got != "KVS error: timeout" {
		t.Errorf("unexpected error string: %s", got)
	}
}

func TestProcessError(t *testing.T) {
	err := &ProcessError{Message: "spawn failed"}
	got := err.Error()
	if got != "process error: spawn failed" {
		t.Errorf("unexpected error string: %s", got)
	}
}
