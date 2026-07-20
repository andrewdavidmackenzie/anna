package annalib

import (
	"fmt"
	"strconv"
	"strings"
)

// ClientConfig holds the minimal configuration a client needs to connect.
type ClientConfig struct {
	RoutingAddresses []string // ZMQ addresses, e.g. ["tcp://10.0.0.1:6450"]
	ClientIP         string   // IP this client binds on
}

// BaseOffset derives the port base offset from the first routing address.
func (c *ClientConfig) BaseOffset() int {
	if len(c.RoutingAddresses) == 0 {
		return 0
	}
	parts := strings.Split(c.RoutingAddresses[0], ":")
	if len(parts) < 3 {
		return 0
	}
	port, err := strconv.Atoi(parts[len(parts)-1])
	if err != nil {
		return 0
	}
	offset := port - kKeyAddressPort
	if offset < 0 {
		return 0
	}
	return offset
}

// RoutingIP extracts the IP from the first routing address.
func (c *ClientConfig) RoutingIP() string {
	if len(c.RoutingAddresses) == 0 {
		return ""
	}
	addr := strings.TrimPrefix(c.RoutingAddresses[0], "tcp://")
	parts := strings.Split(addr, ":")
	if len(parts) >= 1 {
		return parts[0]
	}
	return ""
}

// DefaultClientConfig returns a localhost ClientConfig.
func DefaultClientConfig() *ClientConfig {
	return &ClientConfig{
		RoutingAddresses: []string{fmt.Sprintf("tcp://127.0.0.1:%d", kKeyAddressPort)},
		ClientIP:         "127.0.0.1",
	}
}
