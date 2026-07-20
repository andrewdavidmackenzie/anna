package annalib

import (
	"context"
	"testing"
	"time"

	"github.com/go-zeromq/zmq4"
	"google.golang.org/protobuf/proto"

	kvspb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/kvs"
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

func TestRecvUpdateReceivesPushedValue(t *testing.T) {
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 51200, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	pusher := zmq4.NewPush(context.Background())
	err = pusher.Dial("tcp://127.0.0.1:58350")
	if err != nil {
		t.Fatalf("Dial failed: %v", err)
	}
	defer pusher.Close()
	time.Sleep(100 * time.Millisecond)

	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{
			{Key: "go_test_key", Payload: []byte("go_test_value")},
		},
	}
	payload, err := proto.Marshal(response)
	if err != nil {
		t.Fatalf("Marshal failed: %v", err)
	}
	err = pusher.Send(zmq4.NewMsg(payload))
	if err != nil {
		t.Fatalf("Send failed: %v", err)
	}

	key, val, ok, err := cc.RecvUpdate(5 * time.Second)
	if err != nil {
		t.Fatalf("RecvUpdate failed: %v", err)
	}
	if !ok {
		t.Fatal("expected update, got none")
	}
	if key != "go_test_key" {
		t.Errorf("expected go_test_key, got %s", key)
	}
	if string(val) != "go_test_value" {
		t.Errorf("expected go_test_value, got %s", string(val))
	}

	cached, found := cc.GetCached("go_test_key")
	if !found {
		t.Error("expected key in local cache")
	}
	if string(cached) != "go_test_value" {
		t.Errorf("expected go_test_value in cache, got %s", string(cached))
	}
}

func TestRecvUpdateTimeout(t *testing.T) {
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 51300, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	_, _, ok, err := cc.RecvUpdate(200 * time.Millisecond)
	if err != nil {
		t.Fatalf("RecvUpdate failed: %v", err)
	}
	if ok {
		t.Error("expected no update on timeout")
	}
}
