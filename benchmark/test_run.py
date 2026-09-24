import socket
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import run
from run import Measurements, Response, json_body, json_body_at_size, percentiles, work_chunks


class BenchmarkMathTests(unittest.TestCase):
    def test_percentiles_use_nearest_rank_and_keep_tail_latency(self):
        self.assertEqual(
            percentiles([1.0, 2.0, 3.0, 4.0, 100.0]),
            {"p50_ms": 3.0, "p95_ms": 100.0, "p99_ms": 100.0},
        )

    def test_work_chunks_assign_every_request_once(self):
        self.assertEqual(work_chunks(10, 3), [4, 3, 3])
        self.assertEqual(work_chunks(2, 5), [1, 1])

    def test_contract_mismatch_is_a_benchmark_failure(self):
        stats = Measurements()
        response = Response(200, b"wrong", {}, 2.0)
        self.assertFalse(stats.check("hello", response, 200, lambda body: body == b"Hello, world!"))
        self.assertEqual(stats.error_count, 1)
        self.assertEqual(stats.statuses[200], 1)

    def test_body_limit_payloads_have_exact_byte_lengths_and_valid_json(self):
        for size in (2_097_151, 2_097_152, 2_097_153):
            with self.subTest(size=size):
                payload = json_body_at_size(size)
                self.assertEqual(len(payload), size)
                self.assertEqual(json_body(payload)["password"], "short")
                self.assertTrue(json_body(payload)["username"])

    def test_stalled_postgres_accepts_connection_without_replying(self):
        self.assertTrue(hasattr(run, "StalledPostgres"))
        with run.StalledPostgres() as server:
            with socket.create_connection(("127.0.0.1", server.port), timeout=1) as connection:
                connection.sendall(b"postgres startup")
                self.assertTrue(server.accepted.wait(timeout=1))
                connection.settimeout(0.1)
                with self.assertRaises(socket.timeout):
                    connection.recv(1)

    def test_connect_timeout_case_rejects_failure_without_accepted_connection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "example/posts-crud/target/debug/mads-example-posts-crud"
            binary.parent.mkdir(parents=True)
            binary.write_text("#!/usr/bin/env python3\nimport sys\nprint('MADS140 kind: Connection')\nsys.exit(1)\n")
            binary.chmod(0o755)
            with patch.object(run, "ROOT", root):
                result = run.database_connect_timeout_case("debug")
            if "server_log" in result:
                self.addCleanup(Path(result["server_log"]).unlink, missing_ok=True)
            self.assertFalse(result["passed"])
            self.assertIn("database did not accept a connection", result["error_examples"])


if __name__ == "__main__":
    unittest.main()
