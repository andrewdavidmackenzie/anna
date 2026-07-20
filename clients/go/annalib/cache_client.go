package annalib

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/go-zeromq/zmq4"
	"google.golang.org/protobuf/proto"

	kvspb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/kvs"
	sharedpb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/shared"
)

const (
	kCacheRegistrationPort = 7200
	kCacheUpdatePort       = 7150
)

// CacheClient receives key updates pushed from the KVS during gossip.
type CacheClient struct {
	serverIP      string
	cacheIP       string
	memoryThreads int
	offset        int
	tid           int
	localCache    map[string][]byte
	watchedKeys   []string
	updatePuller  zmq4.Socket
	pushSockets   map[string]zmq4.Socket
	ctx           context.Context
}

// NewCacheClient creates a cache client that listens for gossip updates.
func NewCacheClient(serverIP, cacheIP string, memoryThreads, offset, tid int) (*CacheClient, error) {
	ctx := context.Background()

	bindAddr := fmt.Sprintf("tcp://%s:%d", cacheIP, tid+kCacheUpdatePort+offset)
	puller := zmq4.NewPull(ctx)
	if err := puller.Listen(bindAddr); err != nil {
		return nil, fmt.Errorf("failed to bind cache update puller on %s: %w", bindAddr, err)
	}
	log.Printf("Cache client listening for updates on %s", bindAddr)

	return &CacheClient{
		serverIP:      serverIP,
		cacheIP:       cacheIP,
		memoryThreads: memoryThreads,
		offset:        offset,
		tid:           tid,
		localCache:    make(map[string][]byte),
		updatePuller:  puller,
		pushSockets:   make(map[string]zmq4.Socket),
		ctx:           ctx,
	}, nil
}

// Watch registers interest in keys with all KVS server threads.
func (cc *CacheClient) Watch(keys []string) error {
	cc.watchedKeys = append(cc.watchedKeys, keys...)

	msg := &sharedpb.StringSet{}
	msg.Keys = append(msg.Keys, cc.cacheIP)
	msg.Keys = append(msg.Keys, keys...)

	payload, err := proto.Marshal(msg)
	if err != nil {
		return fmt.Errorf("failed to marshal registration: %w", err)
	}

	for tid := 0; tid < cc.memoryThreads; tid++ {
		addr := fmt.Sprintf("tcp://%s:%d", cc.serverIP, tid+kCacheRegistrationPort+cc.offset)
		sock, ok := cc.pushSockets[addr]
		if !ok {
			sock = zmq4.NewPush(cc.ctx)
			if err := sock.Dial(addr); err != nil {
				return fmt.Errorf("failed to connect to %s: %w", addr, err)
			}
			cc.pushSockets[addr] = sock
		}

		if err := sock.Send(zmq4.NewMsg(payload)); err != nil {
			return fmt.Errorf("failed to send registration to %s: %w", addr, err)
		}
	}

	log.Printf("Registered %d keys with %d KVS threads", len(keys), cc.memoryThreads)
	return nil
}

// RecvUpdate receives the next gossip update from the KVS.
// Returns key, payload, and whether an update was received.
func (cc *CacheClient) RecvUpdate(timeout time.Duration) (string, []byte, bool, error) {
	ctx, cancel := context.WithTimeout(cc.ctx, timeout)
	defer cancel()

	msg, err := cc.updatePuller.Recv()
	if err != nil {
		select {
		case <-ctx.Done():
			return "", nil, false, nil
		default:
			return "", nil, false, fmt.Errorf("recv error: %w", err)
		}
	}

	response := &kvspb.KeyResponse{}
	if err := proto.Unmarshal(msg.Frames[0], response); err != nil {
		return "", nil, false, fmt.Errorf("failed to decode cache update: %w", err)
	}

	for _, tuple := range response.Tuples {
		if len(tuple.Payload) > 0 {
			cc.localCache[tuple.Key] = tuple.Payload
			return tuple.Key, tuple.Payload, true, nil
		}
	}

	return "", nil, false, nil
}

// GetCached reads a value from the local cache.
func (cc *CacheClient) GetCached(key string) ([]byte, bool) {
	val, ok := cc.localCache[key]
	return val, ok
}

// WatchedKeys returns the list of watched keys.
func (cc *CacheClient) WatchedKeys() []string {
	return cc.watchedKeys
}

// Close cleans up ZMQ sockets.
func (cc *CacheClient) Close() error {
	if err := cc.updatePuller.Close(); err != nil {
		return err
	}
	for _, sock := range cc.pushSockets {
		if err := sock.Close(); err != nil {
			return err
		}
	}
	return nil
}
