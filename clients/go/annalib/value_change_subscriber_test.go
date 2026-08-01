package annalib

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/go-zeromq/zmq4"
	"google.golang.org/protobuf/proto"

	kvspb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/kvs"
	sharedpb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/shared"
)

func TestValueChangeSubscriberConstants(t *testing.T) {
	if kCacheRegistrationPort != 6900 {
		t.Errorf("expected registration port 6900, got %d", kCacheRegistrationPort)
	}
	if kCacheUpdatePort != 6850 {
		t.Errorf("expected update port 6850, got %d", kCacheUpdatePort)
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
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 20000, 0)
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
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 20100, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	err = cc.Close()
	if err != nil {
		t.Fatalf("Close failed: %v", err)
	}
}

func TestRecvUpdateReceivesPushedValue(t *testing.T) {
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 20200, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	pusher := zmq4.NewPush(context.Background())
	err = pusher.Dial("tcp://127.0.0.1:27050")
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
	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, 20300, 0)
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

func TestWatchRegistersKeys(t *testing.T) {
	// offset=20500 → registration port: 0+6900+20500 = 27400
	// update port: 0+6850+20500 = 27350 (all below 32768)
	offset := 20500

	// Set up a PULL listener on the registration port so Watch() can connect.
	regPort := 0 + kCacheRegistrationPort + offset // 27700
	regAddr := fmt.Sprintf("tcp://127.0.0.1:%d", regPort)
	puller := zmq4.NewPull(context.Background())
	if err := puller.Listen(regAddr); err != nil {
		t.Fatalf("failed to listen on registration port %d: %v", regPort, err)
	}
	defer puller.Close()

	time.Sleep(100 * time.Millisecond)

	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, offset, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	keys := []string{"key_alpha", "key_beta"}
	if err := cc.Watch(keys); err != nil {
		t.Fatalf("Watch failed: %v", err)
	}

	// Verify watched keys were appended.
	watched := cc.WatchedKeys()
	if len(watched) != 2 {
		t.Fatalf("expected 2 watched keys, got %d", len(watched))
	}
	if watched[0] != "key_alpha" || watched[1] != "key_beta" {
		t.Errorf("unexpected watched keys: %v", watched)
	}

	// Receive the registration message on the PULL socket.
	msg, err := puller.Recv()
	if err != nil {
		t.Fatalf("puller.Recv failed: %v", err)
	}

	var stringSet sharedpb.StringSet
	if err := proto.Unmarshal(msg.Frames[0], &stringSet); err != nil {
		t.Fatalf("failed to unmarshal registration message: %v", err)
	}

	// Registration message format: [cacheIP, key1, key2, ...]
	if len(stringSet.Keys) != 3 {
		t.Fatalf("expected 3 entries in registration (ip + 2 keys), got %d", len(stringSet.Keys))
	}
	if stringSet.Keys[0] != "127.0.0.1" {
		t.Errorf("expected cache IP '127.0.0.1', got %q", stringSet.Keys[0])
	}
	if stringSet.Keys[1] != "key_alpha" || stringSet.Keys[2] != "key_beta" {
		t.Errorf("unexpected keys in registration: %v", stringSet.Keys[1:])
	}
}

func TestWatchMultipleCalls(t *testing.T) {
	// offset=20600 → registration port: 27500, update port: 27450
	offset := 20600

	regPort := 0 + kCacheRegistrationPort + offset
	regAddr := fmt.Sprintf("tcp://127.0.0.1:%d", regPort)
	puller := zmq4.NewPull(context.Background())
	if err := puller.Listen(regAddr); err != nil {
		t.Fatalf("failed to listen on registration port %d: %v", regPort, err)
	}
	defer puller.Close()

	time.Sleep(100 * time.Millisecond)

	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, offset, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	// First watch call.
	if err := cc.Watch([]string{"k1"}); err != nil {
		t.Fatalf("first Watch failed: %v", err)
	}
	// Read the first message.
	if _, err := puller.Recv(); err != nil {
		t.Fatalf("first recv failed: %v", err)
	}

	// Second watch call should append to watched keys and reuse the socket.
	if err := cc.Watch([]string{"k2", "k3"}); err != nil {
		t.Fatalf("second Watch failed: %v", err)
	}
	if _, err := puller.Recv(); err != nil {
		t.Fatalf("second recv failed: %v", err)
	}

	watched := cc.WatchedKeys()
	if len(watched) != 3 {
		t.Fatalf("expected 3 watched keys after two Watch calls, got %d", len(watched))
	}
}

func TestCloseWithPushSockets(t *testing.T) {
	// offset=20700 → registration port: 27600, update port: 27550
	offset := 20700

	regPort := 0 + kCacheRegistrationPort + offset
	regAddr := fmt.Sprintf("tcp://127.0.0.1:%d", regPort)
	puller := zmq4.NewPull(context.Background())
	if err := puller.Listen(regAddr); err != nil {
		t.Fatalf("failed to listen on registration port %d: %v", regPort, err)
	}
	defer puller.Close()

	time.Sleep(100 * time.Millisecond)

	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, offset, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}

	// Watch to create push sockets.
	if err := cc.Watch([]string{"close_test_key"}); err != nil {
		t.Fatalf("Watch failed: %v", err)
	}
	// Drain the message.
	if _, err := puller.Recv(); err != nil {
		t.Fatalf("recv failed: %v", err)
	}

	// Verify push sockets exist before close.
	if len(cc.pushSockets) == 0 {
		t.Fatal("expected at least one push socket after Watch")
	}

	// Close should succeed and close both the update puller and push sockets.
	if err := cc.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}
}

func TestRecvUpdateEmptyPayload(t *testing.T) {
	// offset=20800 → update port: 0+6850+20800 = 27650
	offset := 20800

	cc, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, offset, 0)
	if err != nil {
		t.Fatalf("NewValueChangeSubscriber failed: %v", err)
	}
	defer cc.Close()

	updatePort := 0 + kCacheUpdatePort + offset // 27950
	pusher := zmq4.NewPush(context.Background())
	if err := pusher.Dial(fmt.Sprintf("tcp://127.0.0.1:%d", updatePort)); err != nil {
		t.Fatalf("Dial failed: %v", err)
	}
	defer pusher.Close()
	time.Sleep(100 * time.Millisecond)

	// Send a response with an empty payload.
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{
			{Key: "empty_key", Payload: nil},
		},
	}
	payload, err := proto.Marshal(response)
	if err != nil {
		t.Fatalf("Marshal failed: %v", err)
	}
	if err := pusher.Send(zmq4.NewMsg(payload)); err != nil {
		t.Fatalf("Send failed: %v", err)
	}

	// Should return ok=false because payload is empty.
	_, _, ok, err := cc.RecvUpdate(5 * time.Second)
	if err != nil {
		t.Fatalf("RecvUpdate failed: %v", err)
	}
	if ok {
		t.Error("expected no update for empty payload")
	}
}

func TestNewValueChangeSubscriberBindError(t *testing.T) {
	// Bind the update port first, then try to create a subscriber on the same port.
	// offset=20900 → update port: 0+6850+20900 = 27750
	offset := 20900
	updatePort := 0 + kCacheUpdatePort + offset
	updateAddr := fmt.Sprintf("tcp://127.0.0.1:%d", updatePort)

	blocker := zmq4.NewPull(context.Background())
	if err := blocker.Listen(updateAddr); err != nil {
		t.Fatalf("failed to bind blocker on %s: %v", updateAddr, err)
	}
	defer blocker.Close()

	// Second subscriber on same port should fail.
	_, err := NewValueChangeSubscriber("127.0.0.1", "127.0.0.1", 1, offset, 0)
	if err == nil {
		t.Fatal("expected error when bind port is already in use")
	}
}
