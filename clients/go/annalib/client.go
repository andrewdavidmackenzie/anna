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
	sharedpb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/shared"
)

type transport interface {
	sendRequest(msg []byte, addr string) error
	recvResponse(useKeyAddress bool) ([]byte, error)
	close() error
}

// KVSClient communicates with the Anna KVS via ZeroMQ.
type KVSClient struct {
	routingThreads  []*UserRoutingThread
	rid             int
	ut              *UserThread
	rng             *rand.Rand
	keyAddressCache map[string][]string
	tp              transport
}

type zmqTransport struct {
	timeout          time.Duration
	socketCache      map[string]zmq4.Socket
	keyAddressPuller zmq4.Socket
	responsePuller   zmq4.Socket
	ctx              context.Context
}

// NewKVSClient creates a new KVS client from config and thread ID.
func NewKVSClient(config *Config, tid int) (*KVSClient, error) {
	if config == nil {
		return nil, fmt.Errorf("config must not be nil")
	}
	routingIPs := config.GetRoutingIPs()
	threadCount := config.GetRoutingThreadCount()
	if len(routingIPs) == 0 || threadCount <= 0 {
		return nil, fmt.Errorf("config must have at least one routing IP and positive thread count")
	}
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

	tp := &zmqTransport{
		timeout:          10 * time.Second,
		socketCache:      make(map[string]zmq4.Socket),
		keyAddressPuller: keyAddressPuller,
		responsePuller:   responsePuller,
		ctx:              ctx,
	}

	return &KVSClient{
		routingThreads:  routingThreads,
		rid:             0,
		ut:              ut,
		rng:             rng,
		keyAddressCache: make(map[string][]string),
		tp:              tp,
	}, nil
}

// Close tears down all ZMQ sockets.
func (c *KVSClient) Close() error {
	return c.tp.close()
}

func (t *zmqTransport) close() error {
	for _, sock := range t.socketCache {
		_ = sock.Close()
	}
	_ = t.keyAddressPuller.Close()
	return t.responsePuller.Close()
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

func (t *zmqTransport) getSocket(addr string) (zmq4.Socket, error) {
	if sock, ok := t.socketCache[addr]; ok {
		return sock, nil
	}
	sock := zmq4.NewPush(t.ctx)
	if err := sock.Dial(addr); err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to connect to %s: %v", addr, err)}
	}
	t.socketCache[addr] = sock
	return sock, nil
}

func (t *zmqTransport) sendRequest(msg []byte, addr string) error {
	sock, err := t.getSocket(addr)
	if err != nil {
		return err
	}
	return sock.Send(zmq4.NewMsg(msg))
}

func (t *zmqTransport) recvResponse(useKeyAddress bool) ([]byte, error) {
	sock := t.responsePuller
	if useKeyAddress {
		sock = t.keyAddressPuller
	}

	ctx, cancel := context.WithTimeout(t.ctx, t.timeout)
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

func buildRoutingRequest(requestID, responseAddr, key string) ([]byte, error) {
	request := &kvspb.KeyAddressRequest{
		RequestId:       requestID,
		ResponseAddress: responseAddr,
		Keys:            []string{key},
	}
	return proto.Marshal(request)
}

func parseRoutingResponse(data []byte, key string) ([]string, error) {
	var response kvspb.KeyAddressResponse
	if err := proto.Unmarshal(data, &response); err != nil {
		return nil, fmt.Errorf("failed to decode routing response: %w", err)
	}

	if response.Error != kvspb.AnnaError_NO_ERROR {
		return nil, fmt.Errorf("routing query returned error %d", response.Error)
	}

	var addrs []string
	for _, addr := range response.Addresses {
		if addr.Key == key {
			addrs = append(addrs, addr.Ips...)
		}
	}
	return addrs, nil
}

func (c *KVSClient) queryRouting(key string) []string {
	encoded, err := buildRoutingRequest(c.getRequestID(), c.ut.KeyAddressConnectAddress(), key)
	if err != nil {
		log.Printf("failed to encode routing request: %v", err)
		return nil
	}

	rtThread := c.getRoutingThread()
	if err := c.tp.sendRequest(encoded, rtThread); err != nil {
		log.Printf("failed to send routing request: %v", err)
		return nil
	}

	data, err := c.tp.recvResponse(true)
	if err != nil || data == nil {
		log.Printf("routing query timed out for key %s", key)
		return nil
	}

	addrs, err := parseRoutingResponse(data, key)
	if err != nil {
		log.Printf("%v", err)
		return nil
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

func buildDataRequest(requestID, responseAddr, key string, reqType kvspb.RequestType, latticeType kvspb.LatticeType, payload []byte, cacheSize uint32) ([]byte, error) {
	tuple := &kvspb.KeyTuple{
		Key:              key,
		LatticeType:      latticeType,
		Payload:          payload,
		AddressCacheSize: cacheSize,
	}

	request := &kvspb.KeyRequest{
		RequestId:       requestID,
		ResponseAddress: responseAddr,
		Type:            reqType,
		Tuples:          []*kvspb.KeyTuple{tuple},
	}

	return proto.Marshal(request)
}

func parseDataResponse(data []byte) (*kvspb.KeyResponse, error) {
	var response kvspb.KeyResponse
	if err := proto.Unmarshal(data, &response); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}
	return &response, nil
}

func buildLWWPayload(value string) ([]byte, error) {
	lww := &kvspb.LWWValue{
		Timestamp: generateTimestamp(),
		Value:     []byte(value),
	}
	return proto.Marshal(lww)
}

func parseLWWPayload(payload []byte) (string, error) {
	var lww kvspb.LWWValue
	if err := proto.Unmarshal(payload, &lww); err != nil {
		return "", fmt.Errorf("failed to decode LWW value: %w", err)
	}
	return string(lww.Value), nil
}

func buildSetPayload(values []string) ([]byte, error) {
	setVal := &kvspb.SetValue{
		Values: make([][]byte, len(values)),
	}
	for i, v := range values {
		setVal.Values[i] = []byte(v)
	}
	return proto.Marshal(setVal)
}

func parseSetPayload(payload []byte) ([]string, error) {
	var setVal kvspb.SetValue
	if err := proto.Unmarshal(payload, &setVal); err != nil {
		return nil, fmt.Errorf("failed to decode Set value: %w", err)
	}
	values := make([]string, len(setVal.Values))
	for i, v := range setVal.Values {
		values[i] = string(v)
	}
	return values, nil
}

func (c *KVSClient) sendDataRequest(key string, reqType kvspb.RequestType, latticeType kvspb.LatticeType, payload []byte) (*kvspb.KeyResponse, error) {
	worker, ok := c.getWorkerAddress(key)
	if !ok {
		return nil, &KVSError{Message: fmt.Sprintf("no worker address for key %s", key)}
	}

	var cacheSize uint32
	if cached, ok := c.keyAddressCache[key]; ok {
		cacheSize = uint32(len(cached))
	}

	encoded, err := buildDataRequest(c.getRequestID(), c.ut.ResponseConnectAddress(), key, reqType, latticeType, payload, cacheSize)
	if err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to encode request: %v", err)}
	}

	if err := c.tp.sendRequest(encoded, worker); err != nil {
		return nil, err
	}

	data, err := c.tp.recvResponse(false)
	if err != nil {
		return nil, &KVSError{Message: fmt.Sprintf("failed to receive response: %v", err)}
	}
	if data == nil {
		delete(c.keyAddressCache, key)
		return nil, &KVSError{Message: fmt.Sprintf("%s: request timed out", key)}
	}

	response, err := parseDataResponse(data)
	if err != nil {
		return nil, &KVSError{Message: err.Error()}
	}

	if len(response.Tuples) > 0 && response.Tuples[0].Invalidate {
		delete(c.keyAddressCache, key)
	}

	return response, nil
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

	return parseLWWPayload(tuple.Payload)
}

// Put stores a key-value pair (LWW lattice).
func (c *KVSClient) Put(key, value string) error {
	payload, err := buildLWWPayload(value)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT: %v", err)}
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

	return parseSetPayload(tuple.Payload)
}

// PutSet stores a set of values by key (Set lattice, union semantics).
func (c *KVSClient) PutSet(key string, values []string) error {
	payload, err := buildSetPayload(values)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT_SET: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_SET, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT_SET")
	return err
}

// CausalValue holds the result of a causal GET.
type CausalValue struct {
	VectorClock  map[string]uint32
	Dependencies map[string]map[string]uint32
	Value        string
}

func buildCausalPayload(value string) ([]byte, error) {
	mkc := &kvspb.MultiKeyCausalValue{
		VectorClock: map[string]uint32{"test": 1},
		Dependencies: []*sharedpb.KeyVersion{
			{Key: "dep1", VectorClock: map[string]uint32{"test1": 1}},
		},
		Values: [][]byte{[]byte(value)},
	}
	return proto.Marshal(mkc)
}

func parseCausalPayload(payload []byte) (*CausalValue, error) {
	var mkc kvspb.MultiKeyCausalValue
	if err := proto.Unmarshal(payload, &mkc); err != nil {
		return nil, fmt.Errorf("failed to decode causal value: %w", err)
	}

	deps := make(map[string]map[string]uint32)
	for _, kv := range mkc.Dependencies {
		deps[kv.Key] = kv.VectorClock
	}

	val := ""
	if len(mkc.Values) > 0 {
		val = string(mkc.Values[0])
	}

	return &CausalValue{
		VectorClock:  mkc.VectorClock,
		Dependencies: deps,
		Value:        val,
	}, nil
}

// GetCausal retrieves a value by key (Multi-Key Causal lattice).
func (c *KVSClient) GetCausal(key string) (*CausalValue, error) {
	response, err := c.sendDataRequest(key, kvspb.RequestType_GET, kvspb.LatticeType_NONE, nil)
	if err != nil {
		return nil, err
	}

	tuple, err := validateResponse(response, "GET_CAUSAL")
	if err != nil {
		return nil, err
	}

	return parseCausalPayload(tuple.Payload)
}

// PutCausal stores a value by key (Multi-Key Causal lattice).
func (c *KVSClient) PutCausal(key, value string) error {
	payload, err := buildCausalPayload(value)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT_CAUSAL: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_MULTI_CAUSAL, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT_CAUSAL")
	return err
}

// --- Ordered Set helpers and methods ---

func buildOrderedSetPayload(values []string) ([]byte, error) {
	setVal := &kvspb.SetValue{
		Values: make([][]byte, len(values)),
	}
	for i, v := range values {
		setVal.Values[i] = []byte(v)
	}
	return proto.Marshal(setVal)
}

func parseOrderedSetPayload(payload []byte) ([]string, error) {
	var setVal kvspb.SetValue
	if err := proto.Unmarshal(payload, &setVal); err != nil {
		return nil, fmt.Errorf("failed to decode OrderedSet value: %w", err)
	}
	values := make([]string, len(setVal.Values))
	for i, v := range setVal.Values {
		values[i] = string(v)
	}
	return values, nil
}

// GetOrderedSet retrieves an ordered set of values by key (OrderedSet lattice).
func (c *KVSClient) GetOrderedSet(key string) ([]string, error) {
	response, err := c.sendDataRequest(key, kvspb.RequestType_GET, kvspb.LatticeType_NONE, nil)
	if err != nil {
		return nil, err
	}

	tuple, err := validateResponse(response, "GET_ORDERED_SET")
	if err != nil {
		return nil, err
	}

	return parseOrderedSetPayload(tuple.Payload)
}

// PutOrderedSet stores an ordered set of values by key (OrderedSet lattice).
func (c *KVSClient) PutOrderedSet(key string, values []string) error {
	payload, err := buildOrderedSetPayload(values)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT_ORDERED_SET: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_ORDERED_SET, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT_ORDERED_SET")
	return err
}

// --- Single Causal helpers and methods ---

// SingleCausalValue holds the result of a single-key causal GET.
type SingleCausalValue struct {
	VectorClock map[string]uint32
	Value       string
}

func buildSingleCausalPayload(value string) ([]byte, error) {
	skc := &kvspb.SingleKeyCausalValue{
		VectorClock: map[string]uint32{"test": 1},
		Values:      [][]byte{[]byte(value)},
	}
	return proto.Marshal(skc)
}

func parseSingleCausalPayload(payload []byte) (*SingleCausalValue, error) {
	var skc kvspb.SingleKeyCausalValue
	if err := proto.Unmarshal(payload, &skc); err != nil {
		return nil, fmt.Errorf("failed to decode single causal value: %w", err)
	}

	val := ""
	if len(skc.Values) > 0 {
		val = string(skc.Values[0])
	}

	return &SingleCausalValue{
		VectorClock: skc.VectorClock,
		Value:       val,
	}, nil
}

// GetSingleCausal retrieves a value by key (Single-Key Causal lattice).
func (c *KVSClient) GetSingleCausal(key string) (*SingleCausalValue, error) {
	response, err := c.sendDataRequest(key, kvspb.RequestType_GET, kvspb.LatticeType_NONE, nil)
	if err != nil {
		return nil, err
	}

	tuple, err := validateResponse(response, "GET_SINGLE_CAUSAL")
	if err != nil {
		return nil, err
	}

	return parseSingleCausalPayload(tuple.Payload)
}

// PutSingleCausal stores a value by key (Single-Key Causal lattice).
func (c *KVSClient) PutSingleCausal(key, value string) error {
	payload, err := buildSingleCausalPayload(value)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT_SINGLE_CAUSAL: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_SINGLE_CAUSAL, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT_SINGLE_CAUSAL")
	return err
}

// --- Priority helpers and methods ---

func buildPriorityPayload(priority float64, value string) ([]byte, error) {
	pv := &kvspb.PriorityValue{
		Priority: priority,
		Value:    []byte(value),
	}
	return proto.Marshal(pv)
}

func parsePriorityPayload(payload []byte) (float64, string, error) {
	var pv kvspb.PriorityValue
	if err := proto.Unmarshal(payload, &pv); err != nil {
		return 0, "", fmt.Errorf("failed to decode priority value: %w", err)
	}
	return pv.Priority, string(pv.Value), nil
}

// GetPriority retrieves a priority value by key (Priority lattice).
func (c *KVSClient) GetPriority(key string) (float64, string, error) {
	response, err := c.sendDataRequest(key, kvspb.RequestType_GET, kvspb.LatticeType_NONE, nil)
	if err != nil {
		return 0, "", err
	}

	tuple, err := validateResponse(response, "GET_PRIORITY")
	if err != nil {
		return 0, "", err
	}

	return parsePriorityPayload(tuple.Payload)
}

// PutPriority stores a priority value by key (Priority lattice).
func (c *KVSClient) PutPriority(key string, priority float64, value string) error {
	payload, err := buildPriorityPayload(priority, value)
	if err != nil {
		return &KVSError{Message: fmt.Sprintf("PUT_PRIORITY: %v", err)}
	}

	response, err := c.sendDataRequest(key, kvspb.RequestType_PUT, kvspb.LatticeType_PRIORITY, payload)
	if err != nil {
		return err
	}

	_, err = validateResponse(response, "PUT_PRIORITY")
	return err
}

// ClearCache clears the key-address cache.
func (c *KVSClient) ClearCache() {
	c.keyAddressCache = make(map[string][]string)
}
