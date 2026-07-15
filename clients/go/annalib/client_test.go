package annalib

import (
	"testing"

	"google.golang.org/protobuf/proto"

	kvspb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/kvs"
)

func TestGenerateSeed(t *testing.T) {
	s1 := generateSeed("127.0.0.1", 0)
	s2 := generateSeed("192.168.1.1", 0)
	if s1 == s2 {
		t.Error("different IPs should produce different seeds")
	}

	s3 := generateSeed("127.0.0.1", 0)
	s4 := generateSeed("127.0.0.1", 1)
	if s3 == s4 {
		t.Error("different tids should produce different seeds")
	}
}

func TestGenerateTimestamp(t *testing.T) {
	ts1 := generateTimestamp()
	if ts1 == 0 {
		t.Error("timestamp should be non-zero")
	}
	ts2 := generateTimestamp()
	if ts2 < ts1 {
		t.Error("timestamp should be non-decreasing")
	}
}

func TestLWWValueRoundtrip(t *testing.T) {
	original := &kvspb.LWWValue{
		Timestamp: 12345,
		Value:     []byte("hello world"),
	}

	encoded, err := proto.Marshal(original)
	if err != nil {
		t.Fatalf("failed to encode: %v", err)
	}

	var decoded kvspb.LWWValue
	if err := proto.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if decoded.Timestamp != 12345 {
		t.Errorf("expected timestamp 12345, got %d", decoded.Timestamp)
	}
	if string(decoded.Value) != "hello world" {
		t.Errorf("expected value 'hello world', got '%s'", decoded.Value)
	}
}

func TestSetValueRoundtrip(t *testing.T) {
	original := &kvspb.SetValue{
		Values: [][]byte{[]byte("a"), []byte("b"), []byte("c")},
	}

	encoded, err := proto.Marshal(original)
	if err != nil {
		t.Fatalf("failed to encode: %v", err)
	}

	var decoded kvspb.SetValue
	if err := proto.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if len(decoded.Values) != 3 {
		t.Fatalf("expected 3 values, got %d", len(decoded.Values))
	}
	if string(decoded.Values[0]) != "a" {
		t.Errorf("expected first value 'a', got '%s'", decoded.Values[0])
	}
}

func TestAnnaErrorName(t *testing.T) {
	tests := []struct {
		code int32
		name string
	}{
		{0, "NO_ERROR"},
		{1, "KEY_DNE"},
		{2, "WRONG_THREAD"},
		{3, "TIMEOUT"},
		{4, "LATTICE"},
		{5, "NO_SERVERS"},
		{99, "UNKNOWN"},
	}
	for _, tt := range tests {
		if got := annaErrorName(tt.code); got != tt.name {
			t.Errorf("annaErrorName(%d) = %s, want %s", tt.code, got, tt.name)
		}
	}
}

func TestValidateResponseEmpty(t *testing.T) {
	response := &kvspb.KeyResponse{}
	_, err := validateResponse(response, "TEST")
	if err == nil {
		t.Error("expected error for empty response")
	}
}

func TestValidateResponseError(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{
			{Error: kvspb.AnnaError_KEY_DNE},
		},
	}
	_, err := validateResponse(response, "GET")
	if err == nil {
		t.Error("expected error for KEY_DNE")
	}
	kvsErr, ok := err.(*KVSError)
	if !ok {
		t.Fatalf("expected KVSError, got %T", err)
	}
	if kvsErr.Message != "GET: KEY_DNE" {
		t.Errorf("unexpected error message: %s", kvsErr.Message)
	}
}

func TestValidateResponseSuccess(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{
			{Key: "test_key", Error: kvspb.AnnaError_NO_ERROR},
		},
	}
	tuple, err := validateResponse(response, "GET")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if tuple.Key != "test_key" {
		t.Errorf("expected key 'test_key', got '%s'", tuple.Key)
	}
}

func TestNewKVSClient(t *testing.T) {
	config := DefaultConfig()
	client, err := NewKVSClient(config, 500)
	if err != nil {
		t.Fatalf("NewKVSClient failed: %v", err)
	}
	defer client.Close()

	if len(client.routingThreads) != 1 {
		t.Errorf("expected 1 routing thread, got %d", len(client.routingThreads))
	}
	if client.ut.IP() != "127.0.0.1" {
		t.Errorf("expected IP 127.0.0.1, got %s", client.ut.IP())
	}
}

func TestClearCache(t *testing.T) {
	config := DefaultConfig()
	client, err := NewKVSClient(config, 501)
	if err != nil {
		t.Fatalf("NewKVSClient failed: %v", err)
	}
	defer client.Close()

	client.keyAddressCache["key1"] = []string{"addr1"}
	client.ClearCache()
	if len(client.keyAddressCache) != 0 {
		t.Error("cache should be empty after ClearCache")
	}
}

func TestRequestIDFormat(t *testing.T) {
	config := DefaultConfig()
	client, err := NewKVSClient(config, 502)
	if err != nil {
		t.Fatalf("NewKVSClient failed: %v", err)
	}
	defer client.Close()

	id := client.getRequestID()
	if id != "127.0.0.1:502_1" {
		t.Errorf("unexpected request ID: %s", id)
	}
	id2 := client.getRequestID()
	if id2 != "127.0.0.1:502_2" {
		t.Errorf("unexpected second request ID: %s", id2)
	}
}
