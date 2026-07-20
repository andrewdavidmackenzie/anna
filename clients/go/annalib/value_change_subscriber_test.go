package annalib

import (
	"testing"
)

func TestValueChangeSubscriberConstants(t *testing.T) {
	if kCacheRegistrationPort != 7200 {
		t.Errorf("expected registration port 7200, got %d", kCacheRegistrationPort)
	}
	if kCacheUpdatePort != 7150 {
		t.Errorf("expected update port 7150, got %d", kCacheUpdatePort)
	}
}

func TestValueChangeSubscriberGetCachedMissing(t *testing.T) {
	cc := &ValueChangeSubscriber{
		localCache: make(map[string][]byte),
	}
	_, ok := cc.GetCached("nonexistent")
	if ok {
		t.Error("expected false for missing key")
	}
}

func TestValueChangeSubscriberGetCachedPresent(t *testing.T) {
	cc := &ValueChangeSubscriber{
		localCache: map[string][]byte{
			"test-key": []byte("test-value"),
		},
	}
	val, ok := cc.GetCached("test-key")
	if !ok {
		t.Error("expected true for present key")
	}
	if string(val) != "test-value" {
		t.Errorf("expected test-value, got %s", string(val))
	}
}

func TestValueChangeSubscriberWatchedKeysInitiallyEmpty(t *testing.T) {
	cc := &ValueChangeSubscriber{}
	if len(cc.WatchedKeys()) != 0 {
		t.Error("expected empty watched keys")
	}
}

func TestNewValueChangeSubscriber(t *testing.T) {
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 51000, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	if cc.serverIP != "127.0.0.1" {
		t.Errorf("expected server IP 127.0.0.1, got %s", cc.serverIP)
	}
	if cc.memoryThreads != 1 {
		t.Errorf("expected 1 memory thread, got %d", cc.memoryThreads)
	}
	if len(cc.WatchedKeys()) != 0 {
		t.Error("expected no watched keys initially")
	}
}

func TestValueChangeSubscriberClose(t *testing.T) {
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 51100, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	err = cc.Close()
	if err != nil {
		t.Fatalf("Close failed: %v", err)
	}
}
