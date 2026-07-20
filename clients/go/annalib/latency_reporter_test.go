package annalib

import (
	"context"
	"testing"
	"time"

	"github.com/go-zeromq/zmq4"
	"google.golang.org/protobuf/proto"

	benchpb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/benchmark"
)

func TestNewLatencyReporter(t *testing.T) {
	// Start a listener so the PUSH socket can connect.
	offset := 20000
	port := kMonitoringPort + offset

	ctx := context.Background()
	puller := zmq4.NewPull(ctx)
	if err := puller.Listen("tcp://127.0.0.1:" + itoa(port)); err != nil {
		t.Fatalf("failed to start listener: %v", err)
	}
	defer puller.Close()

	time.Sleep(100 * time.Millisecond)

	lr, err := NewLatencyReporter([]string{"127.0.0.1"}, offset, 900)
	if err != nil {
		t.Fatalf("NewLatencyReporter failed: %v", err)
	}
	defer lr.Close()

	if lr.uid != "client_900" {
		t.Errorf("expected uid client_900, got %s", lr.uid)
	}
	if lr.warmup {
		t.Error("warmup should default to false")
	}
	if len(lr.sockets) != 1 {
		t.Errorf("expected 1 socket, got %d", len(lr.sockets))
	}
}

func TestNewLatencyReporterNoIPs(t *testing.T) {
	_, err := NewLatencyReporter(nil, 0, 0)
	if err == nil {
		t.Error("expected error for empty monitoring IPs")
	}
}

func TestReport(t *testing.T) {
	// Use a unique port offset to avoid conflicts: 6750 + 20100 = 26850
	offset := 20100
	port := kMonitoringPort + offset

	ctx := context.Background()
	puller := zmq4.NewPull(ctx)
	bindAddr := "tcp://127.0.0.1"
	if err := puller.Listen(bindAddr + ":" + itoa(port)); err != nil {
		t.Fatalf("failed to listen on puller: %v", err)
	}
	defer puller.Close()

	// Small delay to let the listener bind.
	time.Sleep(100 * time.Millisecond)

	lr, err := NewLatencyReporter([]string{"127.0.0.1"}, offset, 901)
	if err != nil {
		t.Fatalf("NewLatencyReporter failed: %v", err)
	}
	defer lr.Close()

	keyLats := map[string]float64{
		"key1": 1.5,
		"key2": 2.5,
	}
	if err := lr.Report(100.0, 500.0, keyLats); err != nil {
		t.Fatalf("Report failed: %v", err)
	}

	// Receive the message on the PULL socket.
	msg, err := puller.Recv()
	if err != nil {
		t.Fatalf("puller.Recv failed: %v", err)
	}

	var feedback benchpb.UserFeedback
	if err := proto.Unmarshal(msg.Frames[0], &feedback); err != nil {
		t.Fatalf("failed to unmarshal feedback: %v", err)
	}

	if feedback.Uid != "client_901" {
		t.Errorf("expected uid client_901, got %s", feedback.Uid)
	}
	if feedback.Latency != 100.0 {
		t.Errorf("expected latency 100.0, got %f", feedback.Latency)
	}
	if feedback.Throughput != 500.0 {
		t.Errorf("expected throughput 500.0, got %f", feedback.Throughput)
	}
	if feedback.Warmup {
		t.Error("expected warmup false")
	}
	if feedback.Finish {
		t.Error("expected finish false")
	}
	if len(feedback.KeyLatency) != 2 {
		t.Fatalf("expected 2 key latencies, got %d", len(feedback.KeyLatency))
	}

	// Verify key latencies (order may vary).
	found := make(map[string]float64)
	for _, kl := range feedback.KeyLatency {
		found[kl.Key] = kl.Latency
	}
	if found["key1"] != 1.5 {
		t.Errorf("expected key1 latency 1.5, got %f", found["key1"])
	}
	if found["key2"] != 2.5 {
		t.Errorf("expected key2 latency 2.5, got %f", found["key2"])
	}
}

func TestFinish(t *testing.T) {
	// Use a unique port offset: 6750 + 20200 = 26950
	offset := 20200
	port := kMonitoringPort + offset

	ctx := context.Background()
	puller := zmq4.NewPull(ctx)
	if err := puller.Listen("tcp://127.0.0.1:" + itoa(port)); err != nil {
		t.Fatalf("failed to listen on puller: %v", err)
	}
	defer puller.Close()

	time.Sleep(100 * time.Millisecond)

	lr, err := NewLatencyReporter([]string{"127.0.0.1"}, offset, 902)
	if err != nil {
		t.Fatalf("NewLatencyReporter failed: %v", err)
	}
	defer lr.Close()

	if err := lr.Finish(); err != nil {
		t.Fatalf("Finish failed: %v", err)
	}

	msg, err := puller.Recv()
	if err != nil {
		t.Fatalf("puller.Recv failed: %v", err)
	}

	var feedback benchpb.UserFeedback
	if err := proto.Unmarshal(msg.Frames[0], &feedback); err != nil {
		t.Fatalf("failed to unmarshal feedback: %v", err)
	}

	if !feedback.Finish {
		t.Error("expected finish true")
	}
	if feedback.Uid != "client_902" {
		t.Errorf("expected uid client_902, got %s", feedback.Uid)
	}
}

func TestSetWarmup(t *testing.T) {
	// Use a unique port offset: 6750 + 20300 = 27050
	offset := 20300
	port := kMonitoringPort + offset

	ctx := context.Background()
	puller := zmq4.NewPull(ctx)
	if err := puller.Listen("tcp://127.0.0.1:" + itoa(port)); err != nil {
		t.Fatalf("failed to listen on puller: %v", err)
	}
	defer puller.Close()

	time.Sleep(100 * time.Millisecond)

	lr, err := NewLatencyReporter([]string{"127.0.0.1"}, offset, 903)
	if err != nil {
		t.Fatalf("NewLatencyReporter failed: %v", err)
	}
	defer lr.Close()

	lr.SetWarmup(true)
	if !lr.warmup {
		t.Error("expected warmup to be true after SetWarmup(true)")
	}

	if err := lr.Report(50.0, 100.0, nil); err != nil {
		t.Fatalf("Report failed: %v", err)
	}

	msg, err := puller.Recv()
	if err != nil {
		t.Fatalf("puller.Recv failed: %v", err)
	}

	var feedback benchpb.UserFeedback
	if err := proto.Unmarshal(msg.Frames[0], &feedback); err != nil {
		t.Fatalf("failed to unmarshal feedback: %v", err)
	}

	if !feedback.Warmup {
		t.Error("expected warmup true in feedback")
	}
	if feedback.Latency != 50.0 {
		t.Errorf("expected latency 50.0, got %f", feedback.Latency)
	}

	lr.SetWarmup(false)
	if lr.warmup {
		t.Error("expected warmup to be false after SetWarmup(false)")
	}
}

// itoa is a minimal int-to-string helper to avoid importing strconv in tests.
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	s := ""
	neg := false
	if n < 0 {
		neg = true
		n = -n
	}
	for n > 0 {
		s = string(rune('0'+n%10)) + s
		n /= 10
	}
	if neg {
		s = "-" + s
	}
	return s
}
