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

func serverConfigFile() string {
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
	config := serverConfigFile()

	// Support ANNA_KVS_BIN / ANNA_MONITOR_BIN overrides for dual testing.
	envOverrides := map[string]string{
		"anna-kvs":     "ANNA_KVS_BIN",
		"anna-monitor": "ANNA_MONITOR_BIN",
	}
	for _, proc := range []string{"anna-monitor", "anna-route", "anna-kvs"} {
		binPath := filepath.Join(binDir, proc)
		if envVar, ok := envOverrides[proc]; ok {
			if override := os.Getenv(envVar); override != "" {
				if _, err := os.Stat(override); err != nil {
					t.Fatalf("%s=%s does not exist: %v", envVar, override, err)
				}
				binPath = override
			}
		}
		cmd := exec.Command(binPath, "--config", config)
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

	config := annalib.DefaultClientConfig()
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

	// ORDERED_SET: PUT and GET
	if err := client.PutOrderedSet("go_sys_oset", []string{"alpha", "beta", "gamma"}); err != nil {
		t.Fatalf("PUT_ORDERED_SET failed: %v", err)
	}
	osetVal, err := client.GetOrderedSet("go_sys_oset")
	if err != nil {
		t.Fatalf("GET_ORDERED_SET failed: %v", err)
	}
	if !contains(osetVal, "alpha") || !contains(osetVal, "beta") || !contains(osetVal, "gamma") {
		t.Errorf("GET_ORDERED_SET missing expected values, got %v", osetVal)
	}
	if len(osetVal) != 3 {
		t.Errorf("GET_ORDERED_SET expected 3 elements, got %d", len(osetVal))
	}

	// SINGLE_CAUSAL: PUT and GET
	if err := client.PutSingleCausal("go_sys_sc", "sc_hello"); err != nil {
		t.Fatalf("PUT_SINGLE_CAUSAL failed: %v", err)
	}
	scVal, err := client.GetSingleCausal("go_sys_sc")
	if err != nil {
		t.Fatalf("GET_SINGLE_CAUSAL failed: %v", err)
	}
	if !contains(scVal.Values, "sc_hello") {
		t.Errorf("GET_SINGLE_CAUSAL Values missing 'sc_hello', got %v", scVal.Values)
	}
	if _, ok := scVal.VectorClock["test"]; !ok {
		t.Errorf("GET_SINGLE_CAUSAL VectorClock missing 'test' key, got %v", scVal.VectorClock)
	}

	// MULTI_CAUSAL: PUT and GET
	if err := client.PutCausal("go_sys_mc", "mc_hello"); err != nil {
		t.Fatalf("PUT_CAUSAL failed: %v", err)
	}
	mcVal, err := client.GetCausal("go_sys_mc")
	if err != nil {
		t.Fatalf("GET_CAUSAL failed: %v", err)
	}
	if mcVal.Value != "mc_hello" {
		t.Errorf("GET_CAUSAL Value = %q, want %q", mcVal.Value, "mc_hello")
	}
	if _, ok := mcVal.VectorClock["test"]; !ok {
		t.Errorf("GET_CAUSAL VectorClock missing 'test' key, got %v", mcVal.VectorClock)
	}
	if _, ok := mcVal.Dependencies["dep1"]; !ok {
		t.Errorf("GET_CAUSAL Dependencies missing 'dep1' key, got %v", mcVal.Dependencies)
	}

	// PRIORITY: PUT and GET
	if err := client.PutPriority("go_sys_pri", 1.5, "important"); err != nil {
		t.Fatalf("PUT_PRIORITY failed: %v", err)
	}
	priPriority, priValue, err := client.GetPriority("go_sys_pri")
	if err != nil {
		t.Fatalf("GET_PRIORITY failed: %v", err)
	}
	if priPriority != 1.5 {
		t.Errorf("GET_PRIORITY priority = %f, want 1.5", priPriority)
	}
	if priValue != "important" {
		t.Errorf("GET_PRIORITY value = %q, want %q", priValue, "important")
	}

	// DELETE: PUT, verify, then DELETE
	if err := client.Put("go_sys_del", "to_delete"); err != nil {
		t.Fatalf("PUT for delete test failed: %v", err)
	}
	delVal, err := client.Get("go_sys_del")
	if err != nil {
		t.Fatalf("GET before delete failed: %v", err)
	}
	if delVal != "to_delete" {
		t.Errorf("GET before delete = %q, want %q", delVal, "to_delete")
	}
	if err := client.Delete("go_sys_del"); err != nil {
		t.Fatalf("DELETE failed: %v", err)
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
