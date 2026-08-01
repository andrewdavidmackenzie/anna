package annalib

import (
	"testing"
)

func TestUserThreadAccessors(t *testing.T) {
	ut := NewUserThread("192.168.1.1", 3)
	if ut.IP() != "192.168.1.1" {
		t.Errorf("expected IP 192.168.1.1, got %s", ut.IP())
	}
	if ut.TID() != 3 {
		t.Errorf("expected TID 3, got %d", ut.TID())
	}
}

func TestUserThreadResponseAddresses(t *testing.T) {
	ut := NewUserThread("127.0.0.1", 0)
	if got := ut.ResponseBindAddress(); got != "tcp://0.0.0.0:6600" {
		t.Errorf("ResponseBindAddress: got %s", got)
	}
	if got := ut.ResponseConnectAddress(); got != "tcp://127.0.0.1:6600" {
		t.Errorf("ResponseConnectAddress: got %s", got)
	}
}

func TestUserThreadKeyAddressAddresses(t *testing.T) {
	ut := NewUserThread("127.0.0.1", 0)
	if got := ut.KeyAddressBindAddress(); got != "tcp://0.0.0.0:6650" {
		t.Errorf("KeyAddressBindAddress: got %s", got)
	}
	if got := ut.KeyAddressConnectAddress(); got != "tcp://127.0.0.1:6650" {
		t.Errorf("KeyAddressConnectAddress: got %s", got)
	}
}

func TestUserThreadWithTID(t *testing.T) {
	ut := NewUserThread("10.0.0.1", 5)
	if got := ut.ResponseBindAddress(); got != "tcp://0.0.0.0:6605" {
		t.Errorf("ResponseBindAddress with tid=5: got %s", got)
	}
	if got := ut.KeyAddressBindAddress(); got != "tcp://0.0.0.0:6655" {
		t.Errorf("KeyAddressBindAddress with tid=5: got %s", got)
	}
}

func TestUserRoutingThread(t *testing.T) {
	urt := NewUserRoutingThread("127.0.0.1", 0)
	if got := urt.KeyAddressConnectAddress(); got != "tcp://127.0.0.1:6450" {
		t.Errorf("KeyAddressConnectAddress: got %s", got)
	}
}

func TestUserRoutingThreadWithTID(t *testing.T) {
	urt := NewUserRoutingThread("10.0.0.1", 2)
	if got := urt.KeyAddressConnectAddress(); got != "tcp://10.0.0.1:6452" {
		t.Errorf("KeyAddressConnectAddress with tid=2: got %s", got)
	}
}
