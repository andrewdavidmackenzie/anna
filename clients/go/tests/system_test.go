package tests

import (
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	annalib "github.com/andrewdavidmackenzie/anna/clients/go/annalib"
)

func serverBinaryDir() string {
	root := filepath.Join("..", "..", "..")
	return filepath.Join(root, "server", "cpp", "build", "target", "kvs")
}

func configFile() string {
	root := filepath.Join("..", "..", "..")
	return filepath.Join(root, "conf", "anna-config.yml")
}

func startServers(t *testing.T) {
	t.Helper()

	binDir := serverBinaryDir()
	if _, err := os.Stat(filepath.Join(binDir, "anna-kvs")); os.IsNotExist(err) {
		t.Skip("Server binaries not found, skipping system test")
	}

	path := fmt.Sprintf("%s:%s", os.Getenv("PATH"), binDir)
	config := configFile()

	for _, proc := range []string{"anna-monitor", "anna-route", "anna-kvs"} {
		cmd := exec.Command(proc, "--config", config)
		cmd.Env = append(os.Environ(), "PATH="+path)
		if err := cmd.Start(); err != nil {
			stopServers()
			t.Fatalf("Failed to start %s: %v", proc, err)
		}
		time.Sleep(time.Second)
	}

	deadline := time.Now().Add(30 * time.Second)
	for {
		conn, err := net.DialTimeout("tcp", "127.0.0.1:6450", time.Second)
		if err == nil {
			conn.Close()
			break
		}
		if time.Now().After(deadline) {
			stopServers()
			t.Fatal("Routing tier did not start within 30 seconds")
		}
		time.Sleep(500 * time.Millisecond)
	}
	time.Sleep(3 * time.Second)
}

func stopServers() {
	annalib.Stop()
	time.Sleep(2 * time.Second)
}

func TestSystemKVSClient(t *testing.T) {
	startServers(t)
	defer stopServers()

	config, err := annalib.ReadConfig(configFile())
	if err != nil {
		t.Fatalf("Failed to read config: %v", err)
	}

	client, err := annalib.NewKVSClient(config, 60)
	if err != nil {
		t.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	// PUT and GET a LWW value
	if err := client.Put("go_sys_a", "hello"); err != nil {
		t.Fatalf("PUT failed: %v", err)
	}
	val, err := client.Get("go_sys_a")
	if err != nil {
		t.Fatalf("GET failed: %v", err)
	}
	if val != "hello" {
		t.Errorf("GET returned %q, want %q", val, "hello")
	}

	// Overwrite
	if err := client.Put("go_sys_a", "world"); err != nil {
		t.Fatalf("PUT overwrite failed: %v", err)
	}
	val, err = client.Get("go_sys_a")
	if err != nil {
		t.Fatalf("GET after overwrite failed: %v", err)
	}
	if val != "world" {
		t.Errorf("GET after overwrite returned %q, want %q", val, "world")
	}

	// Multiple keys
	if err := client.Put("go_sys_b", "42"); err != nil {
		t.Fatalf("PUT b failed: %v", err)
	}
	valA, err := client.Get("go_sys_a")
	if err != nil {
		t.Fatalf("GET a failed: %v", err)
	}
	valB, err := client.Get("go_sys_b")
	if err != nil {
		t.Fatalf("GET b failed: %v", err)
	}
	if valA != "world" {
		t.Errorf("GET a = %q, want %q", valA, "world")
	}
	if valB != "42" {
		t.Errorf("GET b = %q, want %q", valB, "42")
	}

	// PUT_SET and GET_SET
	if err := client.PutSet("go_sys_set", []string{"x", "y", "z"}); err != nil {
		t.Fatalf("PUT_SET failed: %v", err)
	}
	setVal, err := client.GetSet("go_sys_set")
	if err != nil {
		t.Fatalf("GET_SET failed: %v", err)
	}
	if !contains(setVal, "x") || !contains(setVal, "y") || !contains(setVal, "z") {
		t.Errorf("GET_SET missing expected values, got %v", setVal)
	}
	if len(setVal) != 3 {
		t.Errorf("GET_SET expected 3 elements, got %d", len(setVal))
	}

	// SET union
	if err := client.PutSet("go_sys_set", []string{"w", "x"}); err != nil {
		t.Fatalf("PUT_SET union failed: %v", err)
	}
	setVal, err = client.GetSet("go_sys_set")
	if err != nil {
		t.Fatalf("GET_SET after union failed: %v", err)
	}
	for _, expected := range []string{"w", "x", "y", "z"} {
		if !contains(setVal, expected) {
			t.Errorf("GET_SET after union missing %q, got %v", expected, setVal)
		}
	}
	if len(setVal) != 4 {
		t.Errorf("Expected 4 elements after union, got %d: %v", len(setVal), setVal)
	}
}

func contains(slice []string, item string) bool {
	for _, s := range slice {
		if s == item {
			return true
		}
	}
	return false
}
