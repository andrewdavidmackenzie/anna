from anna.common import Thread, UserThread, REQUEST_PULLING_BASE_PORT, KEY_ADDRESS_BASE_PORT


class TestThread:
    def test_constructor(self):
        t = Thread("127.0.0.1", 0)
        assert t.get_ip() == "127.0.0.1"
        assert t.get_tid() == 0


class TestUserThread:
    def test_get_request_pull_connect_addr(self):
        t = UserThread("127.0.0.1", 0)
        expected = "tcp://127.0.0.1:" + str(REQUEST_PULLING_BASE_PORT)
        assert t.get_request_pull_connect_addr() == expected

    def test_get_request_pull_bind_addr(self):
        t = UserThread("127.0.0.1", 0)
        expected = "tcp://*:" + str(REQUEST_PULLING_BASE_PORT)
        assert t.get_request_pull_bind_addr() == expected

    def test_get_key_address_connect_addr(self):
        t = UserThread("127.0.0.1", 0)
        expected = "tcp://127.0.0.1:" + str(KEY_ADDRESS_BASE_PORT)
        assert t.get_key_address_connect_addr() == expected

    def test_get_key_address_bind_addr(self):
        t = UserThread("127.0.0.1", 0)
        expected = "tcp://*:" + str(KEY_ADDRESS_BASE_PORT)
        assert t.get_key_address_bind_addr() == expected

    def test_with_offset(self):
        t = UserThread("10.0.0.1", 5)
        expected_port = str(REQUEST_PULLING_BASE_PORT + 5)
        assert t.get_request_pull_connect_addr() == "tcp://10.0.0.1:" + expected_port
