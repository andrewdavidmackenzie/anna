package annalib

import (
	"testing"
)

func TestCacheClientConstants(t *testing.T) {
	if kCacheRegistrationPort != 7200 {
		t.Errorf("expected registration port 7200, got %d", kCacheRegistrationPort)
	}
	if kCacheUpdatePort != 7150 {
		t.Errorf("expected update port 7150, got %d", kCacheUpdatePort)
	}
}

func TestCacheClientGetCachedMissing(t *testing.T) {
	cc := &CacheClient{
		localCache: make(map[string][]byte),
	}
	_, ok := cc.GetCached("nonexistent")
	if ok {
		t.Error("expected false for missing key")
	}
}

func TestCacheClientGetCachedPresent(t *testing.T) {
	cc := &CacheClient{
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

func TestCacheClientWatchedKeysInitiallyEmpty(t *testing.T) {
	cc := &CacheClient{}
	if len(cc.WatchedKeys()) != 0 {
		t.Error("expected empty watched keys")
	}
}
