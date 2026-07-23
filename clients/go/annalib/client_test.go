package annalib

import (
	"fmt"
	"math/rand"
	"testing"

	"google.golang.org/protobuf/proto"

	kvspb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/kvs"
	metadatapb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/metadata"
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
	config := DefaultClientConfig()
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
	config := DefaultClientConfig()
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
	config := DefaultClientConfig()
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

func TestGetRoutingThread(t *testing.T) {
	tp := &mockTransport{}
	client := newTestClient(tp)

	addr := client.getRoutingThread()
	if addr != "tcp://127.0.0.1:6450" {
		t.Errorf("expected routing thread address tcp://127.0.0.1:6450, got %s", addr)
	}
}

func TestGetWorkerAddressFromCache(t *testing.T) {
	tp := &mockTransport{}
	client := newTestClient(tp)

	client.keyAddressCache["cached_key"] = []string{"tcp://10.0.0.1:6800"}
	addr, ok := client.getWorkerAddress("cached_key")
	if !ok {
		t.Fatal("expected to find cached address")
	}
	if addr != "tcp://10.0.0.1:6800" {
		t.Errorf("unexpected address: %s", addr)
	}
}

func TestGetWorkerAddressCacheMiss(t *testing.T) {
	// No cache, queryRouting returns nil (mock has no data), should return false
	tp := &mockTransport{recvData: map[bool][]byte{true: nil}}
	client := newTestClient(tp)

	_, ok := client.getWorkerAddress("missing_key")
	if ok {
		t.Error("expected cache miss with no routing response to return false")
	}
}

func TestBuildRoutingRequest(t *testing.T) {
	data, err := buildRoutingRequest("req_1", "tcp://127.0.0.1:6850", "test_key")
	if err != nil {
		t.Fatalf("buildRoutingRequest failed: %v", err)
	}

	var req kvspb.KeyAddressRequest
	if err := proto.Unmarshal(data, &req); err != nil {
		t.Fatalf("failed to unmarshal: %v", err)
	}
	if req.RequestId != "req_1" {
		t.Errorf("request ID: got %s, want req_1", req.RequestId)
	}
	if req.ResponseAddress != "tcp://127.0.0.1:6850" {
		t.Errorf("response address: got %s", req.ResponseAddress)
	}
	if len(req.Keys) != 1 || req.Keys[0] != "test_key" {
		t.Errorf("keys: got %v", req.Keys)
	}
}

func TestParseRoutingResponse(t *testing.T) {
	response := &kvspb.KeyAddressResponse{
		Error: kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{
			{
				Key: "my_key",
				Ips: []string{"tcp://10.0.0.1:6800", "tcp://10.0.0.2:6800"},
			},
		},
	}
	data, _ := proto.Marshal(response)

	addrs, err := parseRoutingResponse(data, "my_key")
	if err != nil {
		t.Fatalf("parseRoutingResponse failed: %v", err)
	}
	if len(addrs) != 2 {
		t.Fatalf("expected 2 addresses, got %d", len(addrs))
	}
	if addrs[0] != "tcp://10.0.0.1:6800" {
		t.Errorf("unexpected first address: %s", addrs[0])
	}
}

func TestParseRoutingResponseError(t *testing.T) {
	response := &kvspb.KeyAddressResponse{
		Error: kvspb.AnnaError_NO_SERVERS,
	}
	data, _ := proto.Marshal(response)

	_, err := parseRoutingResponse(data, "key")
	if err == nil {
		t.Error("expected error for NO_SERVERS response")
	}
}

func TestParseRoutingResponseWrongKey(t *testing.T) {
	response := &kvspb.KeyAddressResponse{
		Error: kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{
			{Key: "other_key", Ips: []string{"tcp://10.0.0.1:6800"}},
		},
	}
	data, _ := proto.Marshal(response)

	addrs, err := parseRoutingResponse(data, "my_key")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(addrs) != 0 {
		t.Errorf("expected 0 addresses for wrong key, got %d", len(addrs))
	}
}

func TestParseRoutingResponseInvalidData(t *testing.T) {
	_, err := parseRoutingResponse([]byte{0xff, 0xff}, "key")
	if err == nil {
		t.Error("expected error for invalid protobuf data")
	}
}

func TestBuildDataRequest(t *testing.T) {
	data, err := buildDataRequest("req_2", "tcp://127.0.0.1:6800", "key1",
		kvspb.RequestType_PUT, kvspb.LatticeType_LWW, []byte("payload"), 3)
	if err != nil {
		t.Fatalf("buildDataRequest failed: %v", err)
	}

	var req kvspb.KeyRequest
	if err := proto.Unmarshal(data, &req); err != nil {
		t.Fatalf("failed to unmarshal: %v", err)
	}
	if req.RequestId != "req_2" {
		t.Errorf("request ID: got %s", req.RequestId)
	}
	if req.Type != kvspb.RequestType_PUT {
		t.Errorf("request type: got %v", req.Type)
	}
	if len(req.Tuples) != 1 {
		t.Fatalf("expected 1 tuple, got %d", len(req.Tuples))
	}
	if req.Tuples[0].Key != "key1" {
		t.Errorf("tuple key: got %s", req.Tuples[0].Key)
	}
	if req.Tuples[0].AddressCacheSize != 3 {
		t.Errorf("cache size: got %d", req.Tuples[0].AddressCacheSize)
	}
}

func TestParseDataResponse(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{
			{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Payload: []byte("data")},
		},
	}
	data, _ := proto.Marshal(response)

	parsed, err := parseDataResponse(data)
	if err != nil {
		t.Fatalf("parseDataResponse failed: %v", err)
	}
	if len(parsed.Tuples) != 1 || parsed.Tuples[0].Key != "k" {
		t.Errorf("unexpected parsed response: %v", parsed)
	}
}

func TestParseDataResponseInvalid(t *testing.T) {
	_, err := parseDataResponse([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid data")
	}
}

func TestBuildAndParseLWWPayload(t *testing.T) {
	payload, err := buildLWWPayload("hello world")
	if err != nil {
		t.Fatalf("buildLWWPayload failed: %v", err)
	}

	val, err := parseLWWPayload(payload)
	if err != nil {
		t.Fatalf("parseLWWPayload failed: %v", err)
	}
	if val != "hello world" {
		t.Errorf("expected 'hello world', got '%s'", val)
	}
}

func TestParseLWWPayloadInvalid(t *testing.T) {
	_, err := parseLWWPayload([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid LWW payload")
	}
}

func TestBuildAndParseSetPayload(t *testing.T) {
	payload, err := buildSetPayload([]string{"a", "b", "c"})
	if err != nil {
		t.Fatalf("buildSetPayload failed: %v", err)
	}

	values, err := parseSetPayload(payload)
	if err != nil {
		t.Fatalf("parseSetPayload failed: %v", err)
	}
	if len(values) != 3 || values[0] != "a" || values[1] != "b" || values[2] != "c" {
		t.Errorf("unexpected values: %v", values)
	}
}

func TestParseSetPayloadInvalid(t *testing.T) {
	_, err := parseSetPayload([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid Set payload")
	}
}

// mockTransport implements transport for testing without ZMQ.
type mockTransport struct {
	sentMessages []sentMsg
	recvData     map[bool][]byte // useKeyAddress -> response data
	recvErr      error
	sendErr      error
}

type sentMsg struct {
	data []byte
	addr string
}

func (m *mockTransport) sendRequest(msg []byte, addr string) error {
	if m.sendErr != nil {
		return m.sendErr
	}
	m.sentMessages = append(m.sentMessages, sentMsg{data: msg, addr: addr})
	return nil
}

func (m *mockTransport) recvResponse(useKeyAddress bool) ([]byte, error) {
	if m.recvErr != nil {
		return nil, m.recvErr
	}
	return m.recvData[useKeyAddress], nil
}

func (m *mockTransport) close() error { return nil }

func newTestClient(tp transport) *KVSClient {
	return &KVSClient{
		routingThreads:  []*UserRoutingThread{NewUserRoutingThread("127.0.0.1", 0)},
		rid:             0,
		ut:              NewUserThread("127.0.0.1", 0),
		rng:             rand.New(rand.NewSource(42)),
		keyAddressCache: make(map[string][]string),
		tp:              tp,
	}
}

func TestGetWithMock(t *testing.T) {
	lww := &kvspb.LWWValue{Timestamp: 100, Value: []byte("test_value")}
	lwwBytes, _ := proto.Marshal(lww)
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	// Build routing response for address lookup
	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	val, err := client.Get("k")
	if err != nil {
		t.Fatalf("Get failed: %v", err)
	}
	if val != "test_value" {
		t.Errorf("Get returned %q, want %q", val, "test_value")
	}
}

func TestPutWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.Put("k", "some_value")
	if err != nil {
		t.Fatalf("Put failed: %v", err)
	}
	if len(tp.sentMessages) != 2 {
		t.Errorf("expected 2 sent messages (routing + data), got %d", len(tp.sentMessages))
	}
}

func TestGetSetWithMock(t *testing.T) {
	setVal := &kvspb.SetValue{Values: [][]byte{[]byte("a"), []byte("b")}}
	setBytes, _ := proto.Marshal(setVal)
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "s", Error: kvspb.AnnaError_NO_ERROR, Payload: setBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "s", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	vals, err := client.GetSet("s")
	if err != nil {
		t.Fatalf("GetSet failed: %v", err)
	}
	if len(vals) != 2 || vals[0] != "a" || vals[1] != "b" {
		t.Errorf("GetSet returned %v, want [a b]", vals)
	}
}

func TestPutSetWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "s", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "s", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutSet("s", []string{"x", "y"})
	if err != nil {
		t.Fatalf("PutSet failed: %v", err)
	}
}

func TestGetErrorResponse(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_KEY_DNE}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	_, err := client.Get("k")
	if err == nil {
		t.Fatal("expected error for KEY_DNE")
	}
}

func TestGetTimeout(t *testing.T) {
	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	// No data response → simulates timeout (nil)
	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: nil}}
	client := newTestClient(tp)

	_, err := client.Get("k")
	if err == nil {
		t.Fatal("expected error for timeout")
	}
}

func TestSendRequestError(t *testing.T) {
	tp := &mockTransport{sendErr: fmt.Errorf("connection refused")}
	client := newTestClient(tp)

	// Pre-populate cache to skip routing
	client.keyAddressCache["k"] = []string{"tcp://10.0.0.1:6800"}
	_, err := client.Get("k")
	if err == nil {
		t.Fatal("expected error for send failure")
	}
}

func TestInvalidateCacheOnResponse(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Invalidate: true}},
	}
	respBytes, _ := proto.Marshal(response)

	tp := &mockTransport{recvData: map[bool][]byte{false: respBytes}}
	client := newTestClient(tp)
	client.keyAddressCache["k"] = []string{"tcp://10.0.0.1:6800"}

	// Put will succeed but cache should be invalidated
	err := client.Put("k", "val")
	if err != nil {
		t.Fatalf("Put failed: %v", err)
	}
	if _, ok := client.keyAddressCache["k"]; ok {
		t.Error("expected cache to be invalidated")
	}
}

func TestQueryRoutingWithMock(t *testing.T) {
	routingResp := &kvspb.KeyAddressResponse{
		Error: kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{
			{Key: "test_key", Ips: []string{"tcp://10.0.0.1:6800", "tcp://10.0.0.2:6800"}},
		},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes}}
	client := newTestClient(tp)

	addrs := client.queryRouting("test_key")
	if len(addrs) != 2 {
		t.Fatalf("expected 2 addresses, got %d", len(addrs))
	}
}

func TestQueryRoutingSendError(t *testing.T) {
	tp := &mockTransport{sendErr: fmt.Errorf("send failed")}
	client := newTestClient(tp)

	addrs := client.queryRouting("key")
	if addrs != nil {
		t.Errorf("expected nil addresses on send error, got %v", addrs)
	}
}

func TestQueryRoutingTimeout(t *testing.T) {
	tp := &mockTransport{recvData: map[bool][]byte{true: nil}}
	client := newTestClient(tp)

	addrs := client.queryRouting("key")
	if addrs != nil {
		t.Errorf("expected nil addresses on timeout, got %v", addrs)
	}
}

func TestRecvResponseError(t *testing.T) {
	tp := &mockTransport{recvErr: fmt.Errorf("recv error")}
	client := newTestClient(tp)
	client.keyAddressCache["k"] = []string{"tcp://10.0.0.1:6800"}

	_, err := client.Get("k")
	if err == nil {
		t.Fatal("expected error for recv failure")
	}
}

func TestGetNoWorkerAddress(t *testing.T) {
	tp := &mockTransport{recvData: map[bool][]byte{true: nil}}
	client := newTestClient(tp)

	_, err := client.Get("nonexistent")
	if err == nil {
		t.Fatal("expected error when no worker found")
	}
}

func TestPutSetEmptyValues(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "s", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "s", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutSet("s", []string{})
	if err != nil {
		t.Fatalf("PutSet with empty values failed: %v", err)
	}
}

func TestBuildAndParseCausalPayload(t *testing.T) {
	payload, err := buildCausalPayload("hello")
	if err != nil {
		t.Fatalf("buildCausalPayload failed: %v", err)
	}

	cv, err := parseCausalPayload(payload)
	if err != nil {
		t.Fatalf("parseCausalPayload failed: %v", err)
	}
	if cv.Value != "hello" {
		t.Errorf("expected value 'hello', got '%s'", cv.Value)
	}
	if cv.VectorClock["test"] != 1 {
		t.Errorf("expected VC test=1, got %v", cv.VectorClock)
	}
	dep, ok := cv.Dependencies["dep1"]
	if !ok {
		t.Fatal("expected dependency 'dep1'")
	}
	if dep["test1"] != 1 {
		t.Errorf("expected dep VC test1=1, got %v", dep)
	}
}

func TestParseCausalPayloadInvalid(t *testing.T) {
	_, err := parseCausalPayload([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid causal payload")
	}
}

func TestGetCausalWithMock(t *testing.T) {
	payload, _ := buildCausalPayload("causal_value")
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Payload: payload}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	cv, err := client.GetCausal("k")
	if err != nil {
		t.Fatalf("GetCausal failed: %v", err)
	}
	if cv.Value != "causal_value" {
		t.Errorf("expected 'causal_value', got '%s'", cv.Value)
	}
}

func TestPutCausalWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutCausal("k", "test_val")
	if err != nil {
		t.Fatalf("PutCausal failed: %v", err)
	}
}

func TestNewKVSClientNilConfig(t *testing.T) {
	_, err := NewKVSClient(nil, 0)
	if err == nil {
		t.Error("expected error for nil config")
	}
}

func TestNewKVSClientEmptyRouting(t *testing.T) {
	config := &ClientConfig{
		RoutingAddresses: nil,
		ClientIP:         "127.0.0.1",
	}
	_, err := NewKVSClient(config, 0)
	if err == nil {
		t.Error("expected error for empty routing addresses")
	}
}

func TestCloseClient(t *testing.T) {
	config := DefaultClientConfig()
	client, err := NewKVSClient(config, 506)
	if err != nil {
		t.Fatalf("NewKVSClient failed: %v", err)
	}
	if err := client.Close(); err != nil {
		t.Errorf("Close failed: %v", err)
	}
}

// --- Ordered Set tests ---

func TestBuildAndParseOrderedSetPayload(t *testing.T) {
	values := []string{"cherry", "apple", "banana"}
	payload, err := buildOrderedSetPayload(values)
	if err != nil {
		t.Fatalf("buildOrderedSetPayload failed: %v", err)
	}

	result, err := parseOrderedSetPayload(payload)
	if err != nil {
		t.Fatalf("parseOrderedSetPayload failed: %v", err)
	}
	if len(result) != 3 {
		t.Fatalf("expected 3 values, got %d", len(result))
	}
	// Verify order is preserved
	if result[0] != "cherry" || result[1] != "apple" || result[2] != "banana" {
		t.Errorf("order not preserved: got %v", result)
	}
}

func TestParseOrderedSetPayloadInvalid(t *testing.T) {
	_, err := parseOrderedSetPayload([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid OrderedSet payload")
	}
}

func TestGetOrderedSetWithMock(t *testing.T) {
	setVal := &kvspb.SetValue{Values: [][]byte{[]byte("x"), []byte("y"), []byte("z")}}
	setBytes, _ := proto.Marshal(setVal)
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "os", Error: kvspb.AnnaError_NO_ERROR, Payload: setBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "os", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	vals, err := client.GetOrderedSet("os")
	if err != nil {
		t.Fatalf("GetOrderedSet failed: %v", err)
	}
	if len(vals) != 3 || vals[0] != "x" || vals[1] != "y" || vals[2] != "z" {
		t.Errorf("GetOrderedSet returned %v, want [x y z]", vals)
	}
}

func TestPutOrderedSetWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "os", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "os", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutOrderedSet("os", []string{"a", "b", "c"})
	if err != nil {
		t.Fatalf("PutOrderedSet failed: %v", err)
	}
}

// --- Single Causal tests ---

func TestBuildAndParseSingleCausalPayload(t *testing.T) {
	payload, err := buildSingleCausalPayload("hello")
	if err != nil {
		t.Fatalf("buildSingleCausalPayload failed: %v", err)
	}

	scv, err := parseSingleCausalPayload(payload)
	if err != nil {
		t.Fatalf("parseSingleCausalPayload failed: %v", err)
	}
	if len(scv.Values) != 1 || scv.Values[0] != "hello" {
		t.Errorf("expected value 'hello', got '%v'", scv.Values)
	}
	if scv.VectorClock["test"] != 1 {
		t.Errorf("expected VC test=1, got %v", scv.VectorClock)
	}
}

func TestParseSingleCausalPayloadInvalid(t *testing.T) {
	_, err := parseSingleCausalPayload([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid single causal payload")
	}
}

func TestGetSingleCausalWithMock(t *testing.T) {
	payload, _ := buildSingleCausalPayload("sc_value")
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Payload: payload}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	scv, err := client.GetSingleCausal("k")
	if err != nil {
		t.Fatalf("GetSingleCausal failed: %v", err)
	}
	if len(scv.Values) != 1 || scv.Values[0] != "sc_value" {
		t.Errorf("expected 'sc_value', got '%v'", scv.Values)
	}
}

func TestPutSingleCausalWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutSingleCausal("k", "test_val")
	if err != nil {
		t.Fatalf("PutSingleCausal failed: %v", err)
	}
}

// --- Priority tests ---

func TestBuildAndParsePriorityPayload(t *testing.T) {
	payload, err := buildPriorityPayload(3.14, "pi_value")
	if err != nil {
		t.Fatalf("buildPriorityPayload failed: %v", err)
	}

	priority, value, err := parsePriorityPayload(payload)
	if err != nil {
		t.Fatalf("parsePriorityPayload failed: %v", err)
	}
	if priority != 3.14 {
		t.Errorf("expected priority 3.14, got %f", priority)
	}
	if value != "pi_value" {
		t.Errorf("expected value 'pi_value', got '%s'", value)
	}
}

func TestParsePriorityPayloadInvalid(t *testing.T) {
	_, _, err := parsePriorityPayload([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid priority payload")
	}
}

func TestGetPriorityWithMock(t *testing.T) {
	payload, _ := buildPriorityPayload(1.5, "prio_value")
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "p", Error: kvspb.AnnaError_NO_ERROR, Payload: payload}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "p", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	priority, value, err := client.GetPriority("p")
	if err != nil {
		t.Fatalf("GetPriority failed: %v", err)
	}
	if priority != 1.5 {
		t.Errorf("expected priority 1.5, got %f", priority)
	}
	if value != "prio_value" {
		t.Errorf("expected 'prio_value', got '%s'", value)
	}
}

func TestPutPriorityWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "p", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "p", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutPriority("p", 2.5, "test_val")
	if err != nil {
		t.Fatalf("PutPriority failed: %v", err)
	}
}

func TestDeleteWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.Delete("k")
	if err != nil {
		t.Fatalf("Delete failed: %v", err)
	}
}

// --- Metadata stats key format tests ---

func TestMetadataStatsKeyFormat(t *testing.T) {
	key := metadataStatsKey("stats", "10.0.0.1", "192.168.1.1", 0, "MEMORY")
	if key != "ANNA_METADATA|stats|10.0.0.1|192.168.1.1|0|MEMORY" {
		t.Errorf("unexpected key: %s", key)
	}
}

func TestMetadataStatsKeySameIP(t *testing.T) {
	key := metadataStatsKey("stats", "127.0.0.1", "127.0.0.1", 0, "MEMORY")
	if key != "ANNA_METADATA|stats|127.0.0.1|127.0.0.1|0|MEMORY" {
		t.Errorf("unexpected key: %s", key)
	}
}

func TestAccessMetadataKeyFormat(t *testing.T) {
	key := metadataStatsKey("access", "10.0.0.1", "192.168.1.1", 2, "DISK")
	if key != "ANNA_METADATA|access|10.0.0.1|192.168.1.1|2|DISK" {
		t.Errorf("unexpected key: %s", key)
	}
}

func TestSizeMetadataKeyFormat(t *testing.T) {
	key := metadataStatsKey("size", "10.0.0.1", "10.0.0.1", 1, "MEMORY")
	if key != "ANNA_METADATA|size|10.0.0.1|10.0.0.1|1|MEMORY" {
		t.Errorf("unexpected key: %s", key)
	}
}

// --- Protobuf roundtrip tests for metadata types ---

func TestServerThreadStatisticsRoundtrip(t *testing.T) {
	original := &metadatapb.ServerThreadStatistics{
		StorageConsumption: 42000,
		Occupancy:          0.75,
		Epoch:              10,
		AccessCount:        500,
	}

	encoded, err := proto.Marshal(original)
	if err != nil {
		t.Fatalf("failed to encode: %v", err)
	}

	var decoded metadatapb.ServerThreadStatistics
	if err := proto.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if decoded.StorageConsumption != 42000 {
		t.Errorf("expected storage_consumption 42000, got %d", decoded.StorageConsumption)
	}
	if decoded.Occupancy != 0.75 {
		t.Errorf("expected occupancy 0.75, got %f", decoded.Occupancy)
	}
	if decoded.Epoch != 10 {
		t.Errorf("expected epoch 10, got %d", decoded.Epoch)
	}
	if decoded.AccessCount != 500 {
		t.Errorf("expected access_count 500, got %d", decoded.AccessCount)
	}
}

func TestKeyAccessDataRoundtrip(t *testing.T) {
	original := &metadatapb.KeyAccessData{
		Keys: []*metadatapb.KeyAccessData_KeyCount{
			{Key: "key1", AccessCount: 100},
			{Key: "key2", AccessCount: 200},
		},
	}

	encoded, err := proto.Marshal(original)
	if err != nil {
		t.Fatalf("failed to encode: %v", err)
	}

	var decoded metadatapb.KeyAccessData
	if err := proto.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if len(decoded.Keys) != 2 {
		t.Fatalf("expected 2 keys, got %d", len(decoded.Keys))
	}
	if decoded.Keys[0].Key != "key1" || decoded.Keys[0].AccessCount != 100 {
		t.Errorf("unexpected first key: %v", decoded.Keys[0])
	}
	if decoded.Keys[1].Key != "key2" || decoded.Keys[1].AccessCount != 200 {
		t.Errorf("unexpected second key: %v", decoded.Keys[1])
	}
}

func TestKeySizeDataRoundtrip(t *testing.T) {
	original := &metadatapb.KeySizeData{
		KeySizes: []*metadatapb.KeySizeData_KeySize{
			{Key: "big_key", Size: 1024},
			{Key: "small_key", Size: 16},
		},
	}

	encoded, err := proto.Marshal(original)
	if err != nil {
		t.Fatalf("failed to encode: %v", err)
	}

	var decoded metadatapb.KeySizeData
	if err := proto.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if len(decoded.KeySizes) != 2 {
		t.Fatalf("expected 2 key sizes, got %d", len(decoded.KeySizes))
	}
	if decoded.KeySizes[0].Key != "big_key" || decoded.KeySizes[0].Size != 1024 {
		t.Errorf("unexpected first key size: %v", decoded.KeySizes[0])
	}
}

func TestReplicationFactorRoundtrip(t *testing.T) {
	original := &metadatapb.ReplicationFactor{
		Key: "test_key",
		Global: []*metadatapb.ReplicationFactor_ReplicationValue{
			{Tier: metadatapb.Tier_MEMORY, Value: 3},
			{Tier: metadatapb.Tier_DISK, Value: 0},
		},
		Local: []*metadatapb.ReplicationFactor_ReplicationValue{
			{Tier: metadatapb.Tier_MEMORY, Value: 1},
			{Tier: metadatapb.Tier_DISK, Value: 0},
		},
	}

	encoded, err := proto.Marshal(original)
	if err != nil {
		t.Fatalf("failed to encode: %v", err)
	}

	var decoded metadatapb.ReplicationFactor
	if err := proto.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if decoded.Key != "test_key" {
		t.Errorf("expected key 'test_key', got '%s'", decoded.Key)
	}
	if len(decoded.Global) != 2 {
		t.Fatalf("expected 2 global entries, got %d", len(decoded.Global))
	}
	if decoded.Global[0].Tier != metadatapb.Tier_MEMORY || decoded.Global[0].Value != 3 {
		t.Errorf("unexpected global[0]: %v", decoded.Global[0])
	}
	if len(decoded.Local) != 2 {
		t.Fatalf("expected 2 local entries, got %d", len(decoded.Local))
	}
	if decoded.Local[0].Tier != metadatapb.Tier_MEMORY || decoded.Local[0].Value != 1 {
		t.Errorf("unexpected local[0]: %v", decoded.Local[0])
	}
}

// --- GetBytes / metadata helper integration tests with mock ---

func TestParseLWWBytes(t *testing.T) {
	lww := &kvspb.LWWValue{Timestamp: 100, Value: []byte("raw_bytes")}
	lwwBytes, _ := proto.Marshal(lww)

	result, err := parseLWWBytes(lwwBytes)
	if err != nil {
		t.Fatalf("parseLWWBytes failed: %v", err)
	}
	if string(result) != "raw_bytes" {
		t.Errorf("expected 'raw_bytes', got '%s'", result)
	}
}

func TestParseLWWBytesInvalid(t *testing.T) {
	_, err := parseLWWBytes([]byte{0xff, 0xff})
	if err == nil {
		t.Error("expected error for invalid LWW bytes")
	}
}

func TestGetBytesWithMock(t *testing.T) {
	innerPayload := []byte("inner_data")
	lww := &kvspb.LWWValue{Timestamp: 100, Value: innerPayload}
	lwwBytes, _ := proto.Marshal(lww)
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	result, err := client.GetBytes("k")
	if err != nil {
		t.Fatalf("GetBytes failed: %v", err)
	}
	if string(result) != "inner_data" {
		t.Errorf("expected 'inner_data', got '%s'", result)
	}
}

func TestGetStorageStatsWithMock(t *testing.T) {
	stats := &metadatapb.ServerThreadStatistics{
		StorageConsumption: 5000,
		Occupancy:          0.5,
		Epoch:              3,
		AccessCount:        42,
	}
	statsBytes, _ := proto.Marshal(stats)

	lww := &kvspb.LWWValue{Timestamp: 100, Value: statsBytes}
	lwwBytes, _ := proto.Marshal(lww)

	metaKey := "ANNA_METADATA|stats|10.0.0.1|192.168.1.1|0|MEMORY"
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: metaKey, Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: metaKey, Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	result, err := client.GetStorageStats("10.0.0.1", "192.168.1.1", 0, "MEMORY")
	if err != nil {
		t.Fatalf("GetStorageStats failed: %v", err)
	}
	if result.StorageConsumption != 5000 {
		t.Errorf("expected storage_consumption 5000, got %d", result.StorageConsumption)
	}
	if result.Occupancy != 0.5 {
		t.Errorf("expected occupancy 0.5, got %f", result.Occupancy)
	}
	if result.Epoch != 3 {
		t.Errorf("expected epoch 3, got %d", result.Epoch)
	}
	if result.AccessCount != 42 {
		t.Errorf("expected access_count 42, got %d", result.AccessCount)
	}
}

func TestGetKeyAccessStatsWithMock(t *testing.T) {
	data := &metadatapb.KeyAccessData{
		Keys: []*metadatapb.KeyAccessData_KeyCount{
			{Key: "hot_key", AccessCount: 999},
		},
	}
	dataBytes, _ := proto.Marshal(data)

	lww := &kvspb.LWWValue{Timestamp: 100, Value: dataBytes}
	lwwBytes, _ := proto.Marshal(lww)

	metaKey := "ANNA_METADATA|access|10.0.0.1|10.0.0.1|0|MEMORY"
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: metaKey, Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: metaKey, Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	result, err := client.GetKeyAccessStats("10.0.0.1", "10.0.0.1", 0, "MEMORY")
	if err != nil {
		t.Fatalf("GetKeyAccessStats failed: %v", err)
	}
	if len(result.Keys) != 1 || result.Keys[0].Key != "hot_key" || result.Keys[0].AccessCount != 999 {
		t.Errorf("unexpected access data: %v", result)
	}
}

func TestGetKeySizeStatsWithMock(t *testing.T) {
	data := &metadatapb.KeySizeData{
		KeySizes: []*metadatapb.KeySizeData_KeySize{
			{Key: "large_key", Size: 4096},
		},
	}
	dataBytes, _ := proto.Marshal(data)

	lww := &kvspb.LWWValue{Timestamp: 100, Value: dataBytes}
	lwwBytes, _ := proto.Marshal(lww)

	metaKey := "ANNA_METADATA|size|10.0.0.1|10.0.0.1|1|MEMORY"
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: metaKey, Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: metaKey, Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	result, err := client.GetKeySizeStats("10.0.0.1", "10.0.0.1", 1, "MEMORY")
	if err != nil {
		t.Fatalf("GetKeySizeStats failed: %v", err)
	}
	if len(result.KeySizes) != 1 || result.KeySizes[0].Key != "large_key" || result.KeySizes[0].Size != 4096 {
		t.Errorf("unexpected size data: %v", result)
	}
}

// --- parseRoutingAddress edge cases ---

func TestParseRoutingAddressValid(t *testing.T) {
	ip, tid, err := parseRoutingAddress("tcp://10.0.0.1:6450")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ip != "10.0.0.1" {
		t.Errorf("IP = %q, want 10.0.0.1", ip)
	}
	if tid != 0 {
		t.Errorf("tid = %d, want 0", tid)
	}
}

func TestParseRoutingAddressWithOffset(t *testing.T) {
	ip, tid, err := parseRoutingAddress("tcp://10.0.0.1:6460")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ip != "10.0.0.1" {
		t.Errorf("IP = %q, want 10.0.0.1", ip)
	}
	if tid != 10 {
		t.Errorf("tid = %d, want 10", tid)
	}
}

func TestParseRoutingAddressMissingPort(t *testing.T) {
	_, _, err := parseRoutingAddress("tcp://10.0.0.1")
	if err == nil {
		t.Error("expected error for address without port")
	}
}

func TestParseRoutingAddressNonNumericPort(t *testing.T) {
	_, _, err := parseRoutingAddress("tcp://10.0.0.1:abc")
	if err == nil {
		t.Error("expected error for non-numeric port")
	}
}

func TestParseRoutingAddressLowPort(t *testing.T) {
	// Port below kKeyAddressPort (6450) should clamp tid to 0.
	ip, tid, err := parseRoutingAddress("tcp://10.0.0.1:100")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ip != "10.0.0.1" {
		t.Errorf("IP = %q, want 10.0.0.1", ip)
	}
	if tid != 0 {
		t.Errorf("tid = %d, want 0 for low port", tid)
	}
}

func TestNewKVSClientInvalidRoutingAddress(t *testing.T) {
	config := &ClientConfig{
		RoutingAddresses: []string{"invalid-no-port"},
		ClientIP:         "127.0.0.1",
	}
	_, err := NewKVSClient(config, 0)
	if err == nil {
		t.Error("expected error for invalid routing address format")
	}
}

func TestPutReplicationFactorWithMock(t *testing.T) {
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "ANNA_METADATA|replication|my_key", Error: kvspb.AnnaError_NO_ERROR}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error: kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{
			{Key: "ANNA_METADATA|replication|my_key", Ips: []string{"tcp://10.0.0.1:6800"}},
		},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	err := client.PutReplicationFactor("my_key", 3, 1)
	if err != nil {
		t.Fatalf("PutReplicationFactor failed: %v", err)
	}

	// Verify the sent data request contains a valid LWW(ReplicationFactor)
	if len(tp.sentMessages) < 2 {
		t.Fatalf("expected at least 2 sent messages, got %d", len(tp.sentMessages))
	}
	// The second message is the data request
	var req kvspb.KeyRequest
	if err := proto.Unmarshal(tp.sentMessages[1].data, &req); err != nil {
		t.Fatalf("failed to unmarshal data request: %v", err)
	}
	if req.Type != kvspb.RequestType_PUT {
		t.Errorf("expected PUT request type, got %v", req.Type)
	}
	if len(req.Tuples) != 1 {
		t.Fatalf("expected 1 tuple, got %d", len(req.Tuples))
	}
	if req.Tuples[0].LatticeType != kvspb.LatticeType_LWW {
		t.Errorf("expected LWW lattice type, got %v", req.Tuples[0].LatticeType)
	}

	// Decode the LWW wrapper and inner ReplicationFactor
	var lww kvspb.LWWValue
	if err := proto.Unmarshal(req.Tuples[0].Payload, &lww); err != nil {
		t.Fatalf("failed to decode LWW payload: %v", err)
	}
	var rep metadatapb.ReplicationFactor
	if err := proto.Unmarshal(lww.Value, &rep); err != nil {
		t.Fatalf("failed to decode ReplicationFactor: %v", err)
	}
	if rep.Key != "my_key" {
		t.Errorf("expected key 'my_key', got '%s'", rep.Key)
	}
	if len(rep.Global) != 2 || rep.Global[0].Value != 3 {
		t.Errorf("unexpected global replication: %v", rep.Global)
	}
	if len(rep.Local) != 2 || rep.Local[0].Value != 1 {
		t.Errorf("unexpected local replication: %v", rep.Local)
	}
}

func TestGetClusterTopologyWithMock(t *testing.T) {
	topology := &metadatapb.ClusterTopology{
		RoutingThreadCount: 2,
		MemoryThreadCount:  4,
		EbsThreadCount:     1,
	}
	topologyBytes, _ := proto.Marshal(topology)
	lww := &kvspb.LWWValue{Timestamp: 100, Value: topologyBytes}
	lwwBytes, _ := proto.Marshal(lww)
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "ANNA_METADATA|cluster_topology", Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "ANNA_METADATA|cluster_topology", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	result, err := client.GetClusterTopology()
	if err != nil {
		t.Fatalf("GetClusterTopology failed: %v", err)
	}
	if result.RoutingThreadCount != 2 {
		t.Errorf("expected routing_thread_count=2, got %d", result.RoutingThreadCount)
	}
	if result.MemoryThreadCount != 4 {
		t.Errorf("expected memory_thread_count=4, got %d", result.MemoryThreadCount)
	}
	if result.EbsThreadCount != 1 {
		t.Errorf("expected ebs_thread_count=1, got %d", result.EbsThreadCount)
	}
}

func TestGetMonitoringIPsWithMock(t *testing.T) {
	stringSet := &sharedpb.StringSet{Keys: []string{"10.0.0.1", "10.0.0.2"}}
	setBytes, _ := proto.Marshal(stringSet)
	lww := &kvspb.LWWValue{Timestamp: 100, Value: setBytes}
	lwwBytes, _ := proto.Marshal(lww)
	response := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "ANNA_METADATA|monitoring_ips", Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	respBytes, _ := proto.Marshal(response)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "ANNA_METADATA|monitoring_ips", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	tp := &mockTransport{recvData: map[bool][]byte{true: routingBytes, false: respBytes}}
	client := newTestClient(tp)

	result, err := client.GetMonitoringIPs()
	if err != nil {
		t.Fatalf("GetMonitoringIPs failed: %v", err)
	}
	if len(result) != 2 {
		t.Fatalf("expected 2 IPs, got %d", len(result))
	}
	if result[0] != "10.0.0.1" || result[1] != "10.0.0.2" {
		t.Errorf("unexpected IPs: %v", result)
	}
}

func TestWrongThreadRetry(t *testing.T) {
	// First call: routing resolves, then server returns WRONG_THREAD
	wrongThreadResponse := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_WRONG_THREAD}},
	}
	wrongBytes, _ := proto.Marshal(wrongThreadResponse)

	// Second call: success
	lww := &kvspb.LWWValue{Timestamp: 100, Value: []byte("ok")}
	lwwBytes, _ := proto.Marshal(lww)
	okResponse := &kvspb.KeyResponse{
		Tuples: []*kvspb.KeyTuple{{Key: "k", Error: kvspb.AnnaError_NO_ERROR, Payload: lwwBytes}},
	}
	okBytes, _ := proto.Marshal(okResponse)

	routingResp := &kvspb.KeyAddressResponse{
		Error:     kvspb.AnnaError_NO_ERROR,
		Addresses: []*kvspb.KeyAddressResponse_KeyAddress{{Key: "k", Ips: []string{"tcp://10.0.0.1:6800"}}},
	}
	routingBytes, _ := proto.Marshal(routingResp)

	// Use a sequencing mock that returns WRONG_THREAD first, then OK
	tp := &sequencingMockTransport{
		routingData:   routingBytes,
		dataResponses: [][]byte{wrongBytes, okBytes},
	}
	client := newTestClient(tp)
	client.tp = tp

	result, err := client.Get("k")
	if err != nil {
		t.Fatalf("Get with WRONG_THREAD retry failed: %v", err)
	}
	if result != "ok" {
		t.Errorf("expected 'ok', got '%s'", result)
	}
}

// sequencingMockTransport returns different data responses in sequence.
type sequencingMockTransport struct {
	sentMessages  []sentMsg
	routingData   []byte
	dataResponses [][]byte
	dataIndex     int
}

func (m *sequencingMockTransport) sendRequest(msg []byte, addr string) error {
	m.sentMessages = append(m.sentMessages, sentMsg{data: msg, addr: addr})
	return nil
}

func (m *sequencingMockTransport) recvResponse(useKeyAddress bool) ([]byte, error) {
	if useKeyAddress {
		return m.routingData, nil
	}
	if m.dataIndex < len(m.dataResponses) {
		data := m.dataResponses[m.dataIndex]
		m.dataIndex++
		return data, nil
	}
	return nil, nil
}

func (m *sequencingMockTransport) close() error { return nil }
