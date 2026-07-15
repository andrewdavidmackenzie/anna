package annalib

import (
	"context"
	"fmt"
	"hash/fnv"
	"log"
	"math/rand"
	"time"

	"github.com/go-zeromq/zmq4"
	"google.golang.org/protobuf/proto"

	kvspb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/kvs"
)

// KVSClient communicates with the Anna KVS via ZeroMQ.
type KVSClient struct {
	routingThreads   []*UserRoutingThread
	rid              int
	ut               *UserThread
	rng              *rand.Rand
	keyAddressCache  map[string][]string
	timeout          time.Duration
	socketCache      map[string]zmq4.Socket
	keyAddressPuller zmq4.Socket
	responsePuller   zmq4.Socket
	ctx              context.Context
}

// NewKVSClient creates a new KVS client from config and thread ID.
func NewKVSClient(config *Config, tid int) (*KVSClient, error) {
	routingIPs := config.GetRoutingIPs()
	threadCount := config.GetRoutingThreadCount()
	routingThreads := make([]*UserRoutingThread, 0, len(routingIPs)*threadCount)
	for _, ip := range routingIPs {
		for i := 0; i < threadCount; i++ {
			routingThreads = append(routingThreads, NewUserRoutingThread(ip, i))
		}
	}

	ut := NewUserThread(config.GetUserIP(), tid)
	seed := generateSeed(config.GetUserIP(), tid)
	rng := rand.New(rand.NewSource(seed))

	ctx := context.Background()

	keyAddressPuller := zmq4.NewPull(ctx)
	if err := keyAddressPuller.Listen(ut.KeyAddressBindAddress()); err != nil {
		return nil, fmt.Errorf("failed to bind key address puller: %w", err)
	}

	responsePuller := zmq4.NewPull(ctx)
	if err := responsePuller.Listen(ut.ResponseBindAddress()); err != nil {
		_ = keyAddressPuller.Close()
		return nil, fmt.Errorf("failed to bind response puller: %w", err)
	}

	return &KVSClient{
		routingThreads:   routingThreads,
		rid:              0,
		ut:               ut,
		rng:              rng,
		keyAddressCache:  make(map[string][]string),
		timeout:          10 * time.Second,
		socketCache:      make(map[string]zmq4.Socket),
		keyAddressPuller: keyAddressPuller,
		responsePuller:   responsePuller,
		ctx:              ctx,
	}, nil
}

// Close tears down all ZMQ sockets.
func (c *KVSClient) Close() error {
	for _, sock := range c.socketCache {
		_ = sock.Close()
	}
	_ = c.keyAddressPuller.Close()
	return c.responsePuller.Close()
}

func generateSeed(ip string, tid int) int64 {
	now := time.Now()
	seed := now.Unix()*1000 + int64(now.Nanosecond())/1_000_000
	h := fnv.New64a()
	_, _ = h.Write([]byte(ip))
	seed += int64(h.Sum64())
	seed += int64(tid)
	return seed
}

func (c *KVSClient) getRequestID() string {
	c.rid++
	return fmt.Sprintf("%s:%d_%d", c.ut.IP(), c.ut.TID(), c.rid)
}

func (c *KVSClient) getRoutingThread() string {
	idx := c.rng.Intn(len(c.routingThreads))
	return c.routingThreads[idx].KeyAddressConnectAddress()
}

func (c *KVSClient) getSocket(addr string) (zmq4.Socket, error) {
	if sock, ok := c.socketCache[addr]; ok {
		return sock, nil
	}
	sock := zmq4.NewPush(c.ctx)
	if err := sock.Dial(addr); err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to connect to %s: %v", addr, err)}
	}
	c.socketCache[addr] = sock
	return sock, nil
}

func (c *KVSClient) sendRequest(msg []byte, addr string) error {
	sock, err := c.getSocket(addr)
	if err != nil {
		return err
	}
	return sock.Send(zmq4.NewMsg(msg))
}

func (c *KVSClient) recvResponse(useKeyAddress bool) ([]byte, error) {
	sock := c.responsePuller
	if useKeyAddress {
		sock = c.keyAddressPuller
	}

	ctx, cancel := context.WithTimeout(c.ctx, c.timeout)
	defer cancel()

	done := make(chan struct{})
	var msg zmq4.Msg
	var recvErr error

	go func() {
		msg, recvErr = sock.Recv()
		close(done)
	}()

	select {
	case <-done:
		if recvErr != nil {
			return nil, recvErr
		}
		if len(msg.Frames) == 0 {
			return nil, nil
		}
		return msg.Frames[len(msg.Frames)-1], nil
	case <-ctx.Done():
		return nil, nil
	}
}

func (c *KVSClient) queryRouting(key string) []string {
	request := &kvspb.KeyAddressRequest{
		RequestId:       c.getRequestID(),
		ResponseAddress: c.ut.KeyAddressConnectAddress(),
		Keys:            []string{key},
	}

	encoded, err := proto.Marshal(request)
	if err != nil {
		log.Printf("failed to encode routing request: %v", err)
		return nil
	}

	rtThread := c.getRoutingThread()
	if err := c.sendRequest(encoded, rtThread); err != nil {
		log.Printf("failed to send routing request: %v", err)
		return nil
	}

	data, err := c.recvResponse(true)
	if err != nil || data == nil {
		log.Printf("routing query timed out for key %s", key)
		return nil
	}

	var response kvspb.KeyAddressResponse
	if err := proto.Unmarshal(data, &response); err != nil {
		log.Printf("failed to decode routing response: %v", err)
		return nil
	}

	if response.Error != kvspb.AnnaError_NO_ERROR {
		log.Printf("routing query returned error %d", response.Error)
		return nil
	}

	var addrs []string
	for _, addr := range response.Addresses {
		if addr.Key == key {
			addrs = append(addrs, addr.Ips...)
		}
	}
	return addrs
}

func (c *KVSClient) getWorkerAddress(key string) (string, bool) {
	addrs, cached := c.keyAddressCache[key]
	if !cached || len(addrs) == 0 {
		addrs = c.queryRouting(key)
		if len(addrs) == 0 {
			return "", false
		}
		c.keyAddressCache[key] = addrs
	}

	idx := c.rng.Intn(len(addrs))
	return addrs[idx], true
}

func (c *KVSClient) sendDataRequest(key string, reqType kvspb.RequestType, latticeType kvspb.LatticeType, payload []byte) (*kvspb.KeyResponse, error) {
	worker, ok := c.getWorkerAddress(key)
	if !ok {
		return nil, &KVSError{Message: fmt.Sprintf("no worker address for key %s", key)}
	}

	tuple := &kvspb.KeyTuple{
		Key:         key,
		LatticeType: latticeType,
		Payload:     payload,
	}
	if cached, ok := c.keyAddressCache[key]; ok {
		tuple.AddressCacheSize = uint32(len(cached))
	}

	request := &kvspb.KeyRequest{
		RequestId:       c.getRequestID(),
		ResponseAddress: c.ut.ResponseConnectAddress(),
		Type:            reqType,
		Tuples:          []*kvspb.KeyTuple{tuple},
	}

	encoded, err := proto.Marshal(request)
	if err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to encode request: %v", err)}
	}

	if err := c.sendRequest(encoded, worker); err != nil {
		return nil, err
	}

	data, err := c.recvResponse(false)
	if err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to receive response: %v", err)}
	}
	if data == nil {
		delete(c.keyAddressCache, key)
		return nil, &KVSError{Message: fmt.Sprintf("%s: request timed out", key)}
	}

	var response kvspb.KeyResponse
	if err := proto.Unmarshal(data, &response); err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to decode response: %v", err)}
	}

	if len(response.Tuples) > 0 && response.Tuples[0].Invalidate {
		delete(c.keyAddressCache, key)
	}

	return &response, nil
}

func generateTimestamp() uint64 {
	return uint64(time.Now().UnixMilli()) * 10
}

func annaErrorName(code int32) string {
	switch code {
	case 0:
		return "NO_ERROR"
	case 1:
		return "KEY_DNE"
	case 2:
		return "WRONG_THREAD"
	case 3:
		return "TIMEOUT"
	case 4:
		return "LATTICE"
	case 5:
		return "NO_SERVERS"
	default:
		return "UNKNOWN"
	}
}

func validateResponse(response *kvspb.KeyResponse, op string) (*kvspb.KeyTuple, error) {
	if len(response.Tuples) == 0 {
		return nil, &KVSError{Message: fmt.Sprintf("%s: no tuples in response", op)}
	}
	tuple := response.Tuples[0]
	if tuple.Error != kvspb.AnnaError_NO_ERROR {
		return nil, &KVSError{Message: fmt.Sprintf("%s: %s", op, annaErrorName(int32(tuple.Error)))}
	}
	return tuple, nil
}

// Get retrieves a value by key (LWW lattice).
func (c *KVSClient) Get(key string) (string, error) {
	response, err := c.sendDataRequest(key, kvspb.RequestType_GET, kvspb.LatticeType_NONE, nil)
	if err != nil {
		return "", err
	}

	tuple, err := validateResponse(response, "GET")
	if err != nil {
		return "", err
	}

	var lww kvspb.LWWValue
	if err := proto.Unmarshal(tuple.Payload, &lww); err != nil {
		return "", &KVSError{Message: fmt.Sprintf("GET: failed to decode LWW value: %v", err)}
	}
	return string(lww.Value), nil
}

// Put stores a key-value pair (LWW lattice).
func (c *KVSClient) Put(key, value string) error {
	lww := &kvspb.LWWValue{
		Timestamp: generateTimestamp(),
		Value:     []byte(value),
	}
	payload, err := proto.Marshal(lww)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT: failed to encode LWW value: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_LWW, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT")
	return err
}

// GetSet retrieves a set of values by key (Set lattice).
func (c *KVSClient) GetSet(key string) ([]string, error) {
	response, err := c.sendDataRequest(key, kvspb.RequestType_GET, kvspb.LatticeType_NONE, nil)
	if err != nil {
		return nil, err
	}

	tuple, err := validateResponse(response, "GET_SET")
	if err != nil {
		return nil, err
	}

	var setVal kvspb.SetValue
	if err := proto.Unmarshal(tuple.Payload, &setVal); err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("GET_SET: failed to decode Set value: %v", err)}
	}

	values := make([]string, len(setVal.Values))
	for i, v := range setVal.Values {
		values[i] = string(v)
	}
	return values, nil
}

// PutSet stores a set of values by key (Set lattice, union semantics).
func (c *KVSClient) PutSet(key string, values []string) error {
	setVal := &kvspb.SetValue{
		Values: make([][]byte, len(values)),
	}
	for i, v := range values {
		setVal.Values[i] = []byte(v)
	}
	payload, err := proto.Marshal(setVal)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT_SET: failed to encode Set value: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_SET, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT_SET")
	return err
}

// ClearCache clears the key-address cache.
func (c *KVSClient) ClearCache() {
	c.keyAddressCache = make(map[string][]string)
}
