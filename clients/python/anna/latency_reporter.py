import zmq

from . import benchmark_pb2

K_FEEDBACK_REPORT_PORT = 6750


class LatencyReporter:
    """Reports client-observed latency to the anna monitor for SLO enforcement."""

    def __init__(self, monitoring_ips, base_offset=0, tid=0):
        """Create a LatencyReporter.

        Args:
            monitoring_ips: List of monitoring node IP addresses
            base_offset: Port base offset for the cluster
            tid: Client thread ID for UID generation
        """
        self._uid = f"python_client:{tid}"
        self._base_offset = base_offset
        self._warmup = False
        self._monitoring_ips = monitoring_ips
        self._context = zmq.Context(1)
        self._sockets = {}

        for ip in monitoring_ips:
            addr = f"tcp://{ip}:{K_FEEDBACK_REPORT_PORT + base_offset}"
            sock = self._context.socket(zmq.PUSH)
            sock.connect(addr)
            self._sockets[addr] = sock

    def set_warmup(self, warmup):
        self._warmup = warmup

    def report(self, latency_us, throughput, key_latencies=None):
        """Report latency feedback to all monitors.

        Args:
            latency_us: Aggregate latency in microseconds
            throughput: Operations per second
            key_latencies: Optional list of (key, latency_us) tuples
        """
        feedback = benchmark_pb2.UserFeedback()
        feedback.uid = self._uid
        feedback.latency = latency_us
        feedback.throughput = throughput
        feedback.warmup = self._warmup
        if key_latencies:
            for key, lat in key_latencies:
                kl = feedback.key_latency.add()
                kl.key = key
                kl.latency = lat

        payload = feedback.SerializeToString()
        for sock in self._sockets.values():
            sock.send(payload)

    def finish(self):
        """Signal that this client is done reporting."""
        feedback = benchmark_pb2.UserFeedback()
        feedback.uid = self._uid
        feedback.finish = True
        payload = feedback.SerializeToString()
        for sock in self._sockets.values():
            sock.send(payload)

    def close(self):
        for sock in self._sockets.values():
            sock.close()
        self._sockets.clear()
        self._context.term()
