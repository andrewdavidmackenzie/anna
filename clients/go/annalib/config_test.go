package annalib

import (
	"os"
	"testing"
)

func TestDefaultConfig(t *testing.T) {
	config := DefaultConfig()
	if config.GetUserIP() != "127.0.0.1" {
		t.Errorf("expected user IP 127.0.0.1, got %s", config.GetUserIP())
	}
	if config.GetRoutingThreadCount() != 1 {
		t.Errorf("expected routing thread count 1, got %d", config.GetRoutingThreadCount())
	}
	ips := config.GetRoutingIPs()
	if len(ips) != 1 || ips[0] != "127.0.0.1" {
		t.Errorf("expected routing IPs [127.0.0.1], got %v", ips)
	}
}

func TestReadConfig(t *testing.T) {
	config, err := ReadConfig("default-config.yml")
	if err != nil {
		t.Fatalf("ReadConfig failed: %v", err)
	}
	if config.GetUserIP() != "127.0.0.1" {
		t.Errorf("expected user IP 127.0.0.1, got %s", config.GetUserIP())
	}
	if config.GetRoutingThreadCount() < 1 {
		t.Errorf("expected routing thread count >= 1, got %d", config.GetRoutingThreadCount())
	}
}

func TestRoutingIPsNoELB(t *testing.T) {
	config := DefaultConfig()
	config.RoutingELB = nil
	ips := config.GetRoutingIPs()
	if len(ips) != 1 || ips[0] != "127.0.0.1" {
		t.Errorf("expected routing IPs from user.routing, got %v", ips)
	}
}

func TestRoutingIPsWithELB(t *testing.T) {
	config := DefaultConfig()
	config.RoutingELB = []string{"10.0.0.1", "10.0.0.2"}
	ips := config.GetRoutingIPs()
	if len(ips) != 2 || ips[0] != "10.0.0.1" {
		t.Errorf("expected routing IPs from ELB, got %v", ips)
	}
}

func TestReadConfigInvalidYAML(t *testing.T) {
	tmpFile := t.TempDir() + "/bad.yml"
	os.WriteFile(tmpFile, []byte("{{invalid yaml"), 0644)
	_, err := ReadConfig(tmpFile)
	if err == nil {
		t.Error("expected error for invalid YAML")
	}
	cfgErr, ok := err.(*ConfigFileError)
	if !ok {
		t.Fatalf("expected ConfigFileError, got %T", err)
	}
	if cfgErr.Path != tmpFile {
		t.Errorf("expected path %s in error", tmpFile)
	}
}

func TestConfigFileNotFound(t *testing.T) {
	_, err := ReadConfig("nonexistent_file.yml")
	if err == nil {
		t.Fatal("expected error for missing config file")
	}
	cfgErr, ok := err.(*ConfigFileError)
	if !ok {
		t.Fatalf("expected ConfigFileError, got %T", err)
	}
	if cfgErr.Path != "nonexistent_file.yml" {
		t.Errorf("expected path in error, got %s", cfgErr.Path)
	}
}
