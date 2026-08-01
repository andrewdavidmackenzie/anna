package annalib

import (
	"context"
	"fmt"

	"github.com/go-zeromq/zmq4"
	"google.golang.org/protobuf/proto"

	benchpb "github.com/andrewdavidmackenzie/anna/clients/go/annalib/proto/benchmark"
)

const kMonitoringPort = 6953

// LatencyReporter sends UserFeedback to monitoring threads for SLO enforcement.
type LatencyReporter struct {
	uid     string
	warmup  bool
	sockets []zmq4.Socket
	ctx     context.Context
}

// NewLatencyReporter creates a reporter that sends feedback to monitoring threads.
// monitoringIPs are the raw IPs of monitoring nodes; baseOffset adjusts the port;
// tid identifies this client thread.
func NewLatencyReporter(monitoringIPs []string, baseOffset, tid int) (*LatencyReporter, error) {
	if len(monitoringIPs) == 0 {
		return nil, fmt.Errorf("at least one monitoring IP is required")
	}

	ctx := context.Background()
	sockets := make([]zmq4.Socket, 0, len(monitoringIPs))

	for _, ip := range monitoringIPs {
		addr := fmt.Sprintf("tcp://%s:%d", ip, kMonitoringPort+baseOffset)
		sock := zmq4.NewPush(ctx)
		if err := sock.Dial(addr); err != nil {
			// Close any sockets we already opened.
			for _, s := range sockets {
				_ = s.Close()
			}
			return nil, fmt.Errorf("failed to connect to monitoring at %s: %w", addr, err)
		}
		sockets = append(sockets, sock)
	}

	return &LatencyReporter{
		uid:     fmt.Sprintf("client_%d", tid),
		warmup:  false,
		sockets: sockets,
		ctx:     ctx,
	}, nil
}

// Report sends a UserFeedback message with the given latency, throughput, and
// per-key latency observations to all monitoring threads.
func (lr *LatencyReporter) Report(latencyUS, throughput float64, keyLatencies map[string]float64) error {
	feedback := &benchpb.UserFeedback{
		Uid:        lr.uid,
		Latency:    latencyUS,
		Throughput: throughput,
		Warmup:     lr.warmup,
	}

	for key, lat := range keyLatencies {
		feedback.KeyLatency = append(feedback.KeyLatency, &benchpb.UserFeedback_KeyLatency{
			Key:     key,
			Latency: lat,
		})
	}

	data, err := proto.Marshal(feedback)
	if err != nil {
		return fmt.Errorf("failed to marshal feedback: %w", err)
	}

	for _, sock := range lr.sockets {
		if err := sock.Send(zmq4.NewMsg(data)); err != nil {
			return fmt.Errorf("failed to send feedback: %w", err)
		}
	}
	return nil
}

// SetWarmup toggles the warmup flag in subsequent reports.
func (lr *LatencyReporter) SetWarmup(warmup bool) {
	lr.warmup = warmup
}

// Finish sends a UserFeedback with finish=true to signal benchmark completion.
func (lr *LatencyReporter) Finish() error {
	feedback := &benchpb.UserFeedback{
		Uid:    lr.uid,
		Finish: true,
	}
	data, err := proto.Marshal(feedback)
	if err != nil {
		return fmt.Errorf("failed to marshal finish feedback: %w", err)
	}

	for _, sock := range lr.sockets {
		if err := sock.Send(zmq4.NewMsg(data)); err != nil {
			return fmt.Errorf("failed to send finish feedback: %w", err)
		}
	}
	return nil
}

// Close tears down all ZMQ sockets.
func (lr *LatencyReporter) Close() error {
	var lastErr error
	for _, sock := range lr.sockets {
		if err := sock.Close(); err != nil {
			lastErr = err
		}
	}
	return lastErr
}
