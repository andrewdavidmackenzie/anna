import time
import unittest

import zmq

from anna.latency_reporter import LatencyReporter, K_FEEDBACK_REPORT_PORT

# Use offset 20000 to keep ports below 32768 (Linux ephemeral range).
TEST_OFFSET = 20000
TEST_PORT = K_FEEDBACK_REPORT_PORT + TEST_OFFSET


class TestLatencyReporterConstants(unittest.TestCase):
    def test_feedback_report_port(self):
        self.assertEqual(K_FEEDBACK_REPORT_PORT, 6750)


class TestLatencyReporterConstruction(unittest.TestCase):
    def test_construct_with_defaults(self):
        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET)
        try:
            self.assertEqual(reporter._uid, "python_client:0")
            self.assertFalse(reporter._warmup)
            self.assertEqual(len(reporter._sockets), 1)
        finally:
            reporter.close()

    def test_construct_with_tid(self):
        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET, tid=5)
        try:
            self.assertEqual(reporter._uid, "python_client:5")
        finally:
            reporter.close()

    def test_construct_multiple_monitors(self):
        reporter = LatencyReporter(
            ["127.0.0.1", "127.0.0.2"], base_offset=TEST_OFFSET
        )
        try:
            self.assertEqual(len(reporter._sockets), 2)
        finally:
            reporter.close()


class TestLatencyReporterSetWarmup(unittest.TestCase):
    def test_set_warmup(self):
        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET)
        try:
            self.assertFalse(reporter._warmup)
            reporter.set_warmup(True)
            self.assertTrue(reporter._warmup)
            reporter.set_warmup(False)
            self.assertFalse(reporter._warmup)
        finally:
            reporter.close()


class TestLatencyReporterReport(unittest.TestCase):
    def test_report_received(self):
        """Send a report and verify the protobuf arrives on a PULL socket."""
        from anna import benchmark_pb2

        ctx = zmq.Context()
        puller = ctx.socket(zmq.PULL)
        puller.bind(f"tcp://127.0.0.1:{TEST_PORT}")
        time.sleep(0.1)

        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET)
        time.sleep(0.1)

        try:
            reporter.report(42.5, 1000.0)
            time.sleep(0.1)

            poller = zmq.Poller()
            poller.register(puller, zmq.POLLIN)
            events = dict(poller.poll(5000))
            self.assertIn(puller, events)

            data = puller.recv()
            feedback = benchmark_pb2.UserFeedback()
            feedback.ParseFromString(data)

            self.assertEqual(feedback.uid, "python_client:0")
            self.assertAlmostEqual(feedback.latency, 42.5, places=1)
            self.assertAlmostEqual(feedback.throughput, 1000.0, places=1)
            self.assertFalse(feedback.finish)
            self.assertFalse(feedback.warmup)
        finally:
            reporter.close()
            puller.close()
            ctx.term()

    def test_report_with_key_latencies(self):
        """Send a report with per-key latencies."""
        from anna import benchmark_pb2

        # Use a different port (offset+1) to avoid bind conflicts
        port = TEST_PORT + 1
        ctx = zmq.Context()
        puller = ctx.socket(zmq.PULL)
        puller.bind(f"tcp://127.0.0.1:{port}")
        time.sleep(0.1)

        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET + 1)
        time.sleep(0.1)

        try:
            reporter.report(50.0, 500.0, key_latencies=[("k1", 10.0), ("k2", 20.0)])
            time.sleep(0.1)

            poller = zmq.Poller()
            poller.register(puller, zmq.POLLIN)
            events = dict(poller.poll(5000))
            self.assertIn(puller, events)

            data = puller.recv()
            feedback = benchmark_pb2.UserFeedback()
            feedback.ParseFromString(data)

            self.assertEqual(len(feedback.key_latency), 2)
            self.assertEqual(feedback.key_latency[0].key, "k1")
            self.assertAlmostEqual(feedback.key_latency[0].latency, 10.0, places=1)
            self.assertEqual(feedback.key_latency[1].key, "k2")
            self.assertAlmostEqual(feedback.key_latency[1].latency, 20.0, places=1)
        finally:
            reporter.close()
            puller.close()
            ctx.term()

    def test_report_with_warmup(self):
        """Report during warmup phase sets warmup flag."""
        from anna import benchmark_pb2

        port = TEST_PORT + 2
        ctx = zmq.Context()
        puller = ctx.socket(zmq.PULL)
        puller.bind(f"tcp://127.0.0.1:{port}")
        time.sleep(0.1)

        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET + 2)
        reporter.set_warmup(True)
        time.sleep(0.1)

        try:
            reporter.report(100.0, 200.0)
            time.sleep(0.1)

            poller = zmq.Poller()
            poller.register(puller, zmq.POLLIN)
            events = dict(poller.poll(5000))
            self.assertIn(puller, events)

            data = puller.recv()
            feedback = benchmark_pb2.UserFeedback()
            feedback.ParseFromString(data)

            self.assertTrue(feedback.warmup)
        finally:
            reporter.close()
            puller.close()
            ctx.term()


class TestLatencyReporterFinish(unittest.TestCase):
    def test_finish_signal(self):
        """Finish sends a protobuf with finish=True."""
        from anna import benchmark_pb2

        port = TEST_PORT + 3
        ctx = zmq.Context()
        puller = ctx.socket(zmq.PULL)
        puller.bind(f"tcp://127.0.0.1:{port}")
        time.sleep(0.1)

        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET + 3)
        time.sleep(0.1)

        try:
            reporter.finish()
            time.sleep(0.1)

            poller = zmq.Poller()
            poller.register(puller, zmq.POLLIN)
            events = dict(poller.poll(5000))
            self.assertIn(puller, events)

            data = puller.recv()
            feedback = benchmark_pb2.UserFeedback()
            feedback.ParseFromString(data)

            self.assertTrue(feedback.finish)
            self.assertEqual(feedback.uid, "python_client:0")
        finally:
            reporter.close()
            puller.close()
            ctx.term()


class TestLatencyReporterClose(unittest.TestCase):
    def test_close_cleans_up(self):
        reporter = LatencyReporter(["127.0.0.1"], base_offset=TEST_OFFSET + 4)
        reporter.close()
        self.assertEqual(reporter._sockets, {})


if __name__ == "__main__":
    unittest.main()
