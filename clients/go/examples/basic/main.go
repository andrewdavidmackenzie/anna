// Basic example of using the anna Go client library.
//
// This example starts the anna server processes (monitor, route, kvs),
// connects a client, performs basic key-value operations (put, get, delete),
// and then shuts the server down.
//
// Prerequisites:
//
//	The anna server binaries (anna-monitor, anna-route, anna-kvs) must
//	be in your PATH. Build them first with `make server-cpp` or `make server-rust`.
//
// Running:
//
//	go run clients/go/examples/basic/main.go
package main

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"time"

	annalib "github.com/andrewdavidmackenzie/anna/clients/go/annalib"
)

const configTemplate = `monitoring:
  scaling_alert_ip: 127.0.0.1
  ip: 127.0.0.1
routing:
  monitoring:
    - 127.0.0.1
  ip: 127.0.0.1
user:
  monitoring:
    - 127.0.0.1
  routing:
    - 127.0.0.1
  ip: 127.0.0.1
server:
  monitoring:
    - 127.0.0.1
  routing:
    - 127.0.0.1
  seed_ip: 127.0.0.1
  public_ip: 127.0.0.1
  private_ip: 127.0.0.1
  scaling_alert_ip: "NULL"
policy:
  elasticity: false
  selective-rep: false
  tiering: false
disk: %s
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
ports:
  base_offset: 0
`

// waitForRouting waits for the routing tier to accept TCP connections.
func waitForRouting(timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		conn, err := net.DialTimeout("tcp", "127.0.0.1:6450", time.Second)
		if err == nil {
			conn.Close()
			time.Sleep(time.Second)
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("routing tier did not start within %v", timeout)
		}
		time.Sleep(500 * time.Millisecond)
	}
}

func main() {
	// Create a temporary config
	workDir, err := os.MkdirTemp("", "anna_example_")
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to create temp dir: %v\n", err)
		os.Exit(1)
	}
	defer os.RemoveAll(workDir)

	diskDir := filepath.Join(workDir, "disk")
	os.MkdirAll(diskDir, 0o755)

	configPath := filepath.Join(workDir, "config.yml")
	config := fmt.Sprintf(configTemplate, diskDir)
	if err := os.WriteFile(configPath, []byte(config), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "Failed to write config: %v\n", err)
		os.Exit(1)
	}

	// Start the anna server
	fmt.Println("Starting anna server...")
	count, err := annalib.Start(configPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to start: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("  Started %d processes\n", count)

	defer func() {
		fmt.Println("\nStopping anna server...")
		killed, _ := annalib.Stop()
		fmt.Printf("  Stopped %d processes\n", killed)
	}()

	if err := waitForRouting(30 * time.Second); err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}

	// Connect a client
	clientConfig := annalib.DefaultClientConfig()
	client, err := annalib.NewKVSClient(clientConfig, 50)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to create client: %v\n", err)
		os.Exit(1)
	}
	defer client.Close()

	// PUT a value
	fmt.Println("\nPUT greeting = hello")
	if err := client.Put("greeting", "hello"); err != nil {
		fmt.Fprintf(os.Stderr, "PUT failed: %v\n", err)
		os.Exit(1)
	}

	// GET it back
	val, err := client.Get("greeting")
	if err != nil {
		fmt.Fprintf(os.Stderr, "GET failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("GET greeting = %s\n", val)

	// Overwrite the value
	fmt.Println("\nPUT greeting = hello world")
	if err := client.Put("greeting", "hello world"); err != nil {
		fmt.Fprintf(os.Stderr, "PUT overwrite failed: %v\n", err)
		os.Exit(1)
	}

	val, err = client.Get("greeting")
	if err != nil {
		fmt.Fprintf(os.Stderr, "GET failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("GET greeting = %s\n", val)

	// PUT a second key
	fmt.Println("\nPUT count = 42")
	if err := client.Put("count", "42"); err != nil {
		fmt.Fprintf(os.Stderr, "PUT count failed: %v\n", err)
		os.Exit(1)
	}

	// DELETE the first key
	fmt.Println("\nDELETE greeting")
	if err := client.Delete("greeting"); err != nil {
		fmt.Fprintf(os.Stderr, "DELETE failed: %v\n", err)
		os.Exit(1)
	}

	// Verify deletion
	_, err = client.Get("greeting")
	if err != nil {
		fmt.Println("GET greeting = (deleted)")
	} else {
		fmt.Printf("GET greeting = %s (unexpected)\n", val)
	}

	// GET the remaining key
	val, err = client.Get("count")
	if err != nil {
		fmt.Fprintf(os.Stderr, "GET count failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("GET count = %s\n", val)

	fmt.Println("\nDone!")
}
