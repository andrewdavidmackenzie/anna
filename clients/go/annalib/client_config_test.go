package annalib

import (
	"testing"
)

// --- BaseOffset tests ---

func TestBaseOffsetEmptyAddresses(t *testing.T) {
	c := &ClientConfig{RoutingAddresses: nil}
	if got := c.BaseOffset(); got != 0 {
		t.Errorf("BaseOffset() = %d, want 0 for empty addresses", got)
	}
}

func TestBaseOffsetTooFewParts(t *testing.T) {
	// Address with fewer than 3 colon-separated parts (no port).
	c := &ClientConfig{RoutingAddresses: []string{"tcp//127.0.0.1"}}
	if got := c.BaseOffset(); got != 0 {
		t.Errorf("BaseOffset() = %d, want 0 for malformed address", got)
	}
}

func TestBaseOffsetNonNumericPort(t *testing.T) {
	c := &ClientConfig{RoutingAddresses: []string{"tcp://127.0.0.1:abc"}}
	if got := c.BaseOffset(); got != 0 {
		t.Errorf("BaseOffset() = %d, want 0 for non-numeric port", got)
	}
}

func TestBaseOffsetNegativeOffset(t *testing.T) {
	// Port below kKeyAddressPort (6450) yields a negative offset, clamped to 0.
	c := &ClientConfig{RoutingAddresses: []string{"tcp://127.0.0.1:100"}}
	if got := c.BaseOffset(); got != 0 {
		t.Errorf("BaseOffset() = %d, want 0 for port below base", got)
	}
}

func TestBaseOffsetZero(t *testing.T) {
	// Port exactly equal to kKeyAddressPort (6450) yields offset 0.
	c := &ClientConfig{RoutingAddresses: []string{"tcp://127.0.0.1:6450"}}
	if got := c.BaseOffset(); got != 0 {
		t.Errorf("BaseOffset() = %d, want 0", got)
	}
}

func TestBaseOffsetPositive(t *testing.T) {
	// Port 6460 yields offset 10 (6460 - 6450).
	c := &ClientConfig{RoutingAddresses: []string{"tcp://127.0.0.1:6460"}}
	if got := c.BaseOffset(); got != 10 {
		t.Errorf("BaseOffset() = %d, want 10", got)
	}
}

func TestBaseOffsetUsesFirstAddress(t *testing.T) {
	// Only the first routing address is used; second is ignored.
	c := &ClientConfig{RoutingAddresses: []string{"tcp://10.0.0.1:6500", "tcp://10.0.0.2:7000"}}
	if got := c.BaseOffset(); got != 50 {
		t.Errorf("BaseOffset() = %d, want 50 (6500 - 6450)", got)
	}
}

// --- RoutingIP tests ---

func TestRoutingIPEmptyAddresses(t *testing.T) {
	c := &ClientConfig{RoutingAddresses: nil}
	if got := c.RoutingIP(); got != "" {
		t.Errorf("RoutingIP() = %q, want empty for no addresses", got)
	}
}

func TestRoutingIPStandard(t *testing.T) {
	c := &ClientConfig{RoutingAddresses: []string{"tcp://10.0.0.1:6450"}}
	if got := c.RoutingIP(); got != "10.0.0.1" {
		t.Errorf("RoutingIP() = %q, want 10.0.0.1", got)
	}
}

func TestRoutingIPLocalhost(t *testing.T) {
	c := &ClientConfig{RoutingAddresses: []string{"tcp://127.0.0.1:6450"}}
	if got := c.RoutingIP(); got != "127.0.0.1" {
		t.Errorf("RoutingIP() = %q, want 127.0.0.1", got)
	}
}

func TestRoutingIPNoPrefixStillWorks(t *testing.T) {
	// Without tcp:// prefix, TrimPrefix is a no-op; IP should still parse.
	c := &ClientConfig{RoutingAddresses: []string{"192.168.1.1:6450"}}
	if got := c.RoutingIP(); got != "192.168.1.1" {
		t.Errorf("RoutingIP() = %q, want 192.168.1.1", got)
	}
}

func TestRoutingIPUsesFirstAddress(t *testing.T) {
	c := &ClientConfig{RoutingAddresses: []string{"tcp://10.0.0.1:6450", "tcp://10.0.0.2:6450"}}
	if got := c.RoutingIP(); got != "10.0.0.1" {
		t.Errorf("RoutingIP() = %q, want 10.0.0.1", got)
	}
}

// --- DefaultClientConfig test ---

func TestDefaultClientConfigValues(t *testing.T) {
	c := DefaultClientConfig()
	if c.ClientIP != "127.0.0.1" {
		t.Errorf("ClientIP = %q, want 127.0.0.1", c.ClientIP)
	}
	if len(c.RoutingAddresses) != 1 {
		t.Fatalf("expected 1 routing address, got %d", len(c.RoutingAddresses))
	}
	if c.BaseOffset() != 0 {
		t.Errorf("default config BaseOffset = %d, want 0", c.BaseOffset())
	}
	if c.RoutingIP() != "127.0.0.1" {
		t.Errorf("default config RoutingIP = %q, want 127.0.0.1", c.RoutingIP())
	}
}
