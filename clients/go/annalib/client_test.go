package annalib

import (
	"fmt"
	"math/rand"
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
	config := DefaultConfig()
	config.User.Routing = nil
	_, err := NewKVSClient(config, 0)
	if err == nil {
		t.Error("expected error for empty routing IPs")
	}
}

func TestNewKVSClientZeroThreads(t *testing.T) {
	config := DefaultConfig()
	config.Threads.Routing = 0
	_, err := NewKVSClient(config, 0)
	if err == nil {
		t.Error("expected error for zero routing threads")
	}
}

func TestCloseClient(t *testing.T) {
	config := DefaultConfig()
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
