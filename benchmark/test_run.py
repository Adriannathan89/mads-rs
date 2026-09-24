import os
import socket
import socketserver
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from urllib.parse import parse_qs, urlsplit
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

    def test_statement_timeout_url_preserves_existing_database_options(self):
        self.assertTrue(hasattr(run, "statement_timeout_url"))
        url = run.statement_timeout_url("postgres://user:pass@127.0.0.1:5432/test?sslmode=disable", 2000)
        self.assertEqual(
            parse_qs(urlsplit(url).query),
            {"sslmode": ["disable"], "options": ["-c statement_timeout=2000"]},
        )

    def test_proxy_database_url_preserves_credentials_and_query(self):
        self.assertTrue(hasattr(run, "proxy_database_url"))
        self.assertEqual(
            run.proxy_database_url("postgres://user:secret@db.example:5433/posts?sslmode=disable", 6543),
            "postgres://user:secret@127.0.0.1:6543/posts?sslmode=disable",
        )

    def test_proxy_stalls_existing_connection_and_resumes_same_connection(self):
        self.assertTrue(hasattr(run, "StallablePostgresProxy"))

        class Echo(socketserver.BaseRequestHandler):
            def handle(self):
                self.request.recv(4)
                self.request.sendall(b"pong")

        with socketserver.ThreadingTCPServer(("127.0.0.1", 0), Echo) as target:
            worker = threading.Thread(target=target.serve_forever, daemon=True)
            worker.start()
            try:
                with run.StallablePostgresProxy("127.0.0.1", target.server_address[1]) as proxy:
                    with socket.create_connection(("127.0.0.1", proxy.port), timeout=1) as client:
                        proxy.pause_downstream()
                        client.sendall(b"ping")
                        self.assertTrue(proxy.downstream_blocked.wait(timeout=1))
                        client.settimeout(0.1)
                        with self.assertRaises(socket.timeout):
                            client.recv(4)
                        proxy.resume_downstream()
                        self.assertEqual(client.recv(4), b"pong")
                        self.assertTrue(hasattr(proxy, "accepted_connections"))
                        self.assertEqual(proxy.accepted_connections, 1)
            finally:
                target.shutdown()
                worker.join(timeout=1)

    @unittest.skipUnless(os.environ.get("MADS_TEST_BENCH_DATABASE_URL"), "requires isolated PostgreSQL")
    def test_postgres_table_lock_is_visible_and_released(self):
        self.assertTrue(hasattr(run, "PostgresTableLock"))
        url = os.environ["MADS_TEST_BENCH_DATABASE_URL"]

        def lock_count():
            result = subprocess.run(
                ["psql", "-XAt", "-v", "ON_ERROR_STOP=1", url, "-c",
                 "SELECT count(*) FROM pg_locks WHERE relation='posts'::regclass "
                 "AND mode='AccessExclusiveLock' AND granted"],
                capture_output=True, text=True, check=True,
            )
            return int(result.stdout.strip())

        with run.PostgresTableLock(url):
            self.assertGreaterEqual(lock_count(), 1)
        self.assertEqual(lock_count(), 0)

    @unittest.skipUnless(
        os.environ.get("MADS_TEST_BENCH_DATABASE_URL") and os.environ.get("MADS_TEST_BENCH_APP_ROOT"),
        "requires isolated PostgreSQL and a built posts example",
    )
    def test_http_survives_query_timeout_and_recovers(self):
        self.assertTrue(hasattr(run, "database_query_timeout_recovery_case"))
        with patch.object(run, "ROOT", Path(os.environ["MADS_TEST_BENCH_APP_ROOT"])):
            result = run.database_query_timeout_recovery_case(
                "debug", os.environ["MADS_TEST_BENCH_DATABASE_URL"]
            )
        self.assertTrue(result["passed"], result)

    @unittest.skipUnless(
        os.environ.get("MADS_TEST_BENCH_DATABASE_URL") and os.environ.get("MADS_TEST_BENCH_APP_ROOT"),
        "requires isolated PostgreSQL and a built posts example",
    )
    def test_http_survives_tcp_stall_and_recovers(self):
        self.assertTrue(hasattr(run, "database_tcp_stall_recovery_case"))
        with patch.object(run, "ROOT", Path(os.environ["MADS_TEST_BENCH_APP_ROOT"])):
            result = run.database_tcp_stall_recovery_case(
                "debug", os.environ["MADS_TEST_BENCH_DATABASE_URL"]
            )
        self.assertTrue(result["passed"], result)

    def test_recovery_cases_require_an_isolated_database_url(self):
        for case in ("database-query-timeout-recovery", "database-tcp-stall-recovery"):
            with self.subTest(case=case):
                environment = os.environ.copy()
                environment.pop("BENCH_DATABASE_URL", None)
                result = subprocess.run(
                    [sys.executable, str(Path(run.__file__)), "--case", case],
                    env=environment, capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 2)
                self.assertIn("requires BENCH_DATABASE_URL", result.stderr)


if __name__ == "__main__":
    unittest.main()
