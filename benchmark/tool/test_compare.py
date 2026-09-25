import json
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread

from compare import body_limit_probe, contract_checks, measure, median_summary, warmup_size


class ComparisonSummaryTests(unittest.TestCase):
    def test_summary_uses_median_not_fastest_run(self):
        runs = [
            {"passed": True, "responses_per_second": 100.0, "p50_ms": 4.0, "p95_ms": 12.0, "p99_ms": 20.0},
            {"passed": True, "responses_per_second": 300.0, "p50_ms": 2.0, "p95_ms": 8.0, "p99_ms": 16.0},
            {"passed": True, "responses_per_second": 200.0, "p50_ms": 3.0, "p95_ms": 10.0, "p99_ms": 18.0},
        ]
        self.assertEqual(median_summary(runs), {
            "runs": 3, "median_responses_per_second": 200.0,
            "median_p50_ms": 3.0, "median_p95_ms": 10.0, "median_p99_ms": 18.0, "passed": True,
        })

    def test_failed_run_cannot_be_reported_as_passed(self):
        runs = [
            {"passed": True, "responses_per_second": 100.0, "p50_ms": 4.0, "p95_ms": 12.0, "p99_ms": 20.0},
            {"passed": False, "responses_per_second": 200.0, "p50_ms": 3.0, "p95_ms": 10.0, "p99_ms": 18.0},
        ]
        self.assertFalse(median_summary(runs)["passed"])

    def test_warmup_reaches_measured_concurrency_without_exceeding_workload(self):
        self.assertEqual(warmup_size(800, 32), (320, 32))
        self.assertEqual(warmup_size(20, 4), (20, 4))

    def test_body_limit_probe_records_oversized_body_rejection(self):
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            oversized_seen = False

            def log_message(self, *_args):
                pass

            def do_GET(self):
                if self.headers.get("Authorization") == "bearer test-token":
                    status = 200
                    body = b'{"id":1,"username":"demo"}'
                else:
                    status = 401
                    body = b'{"error":{"code":"unauthorized","message":"authentication was rejected"}}'
                self.send_response(status)
                if status == 401:
                    self.send_header("WWW-Authenticate", "Bearer")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                size = int(self.headers["Content-Length"])
                payload = self.rfile.read(size)
                close_connection = False
                if self.headers.get("Content-Type") != "application/json":
                    status, body = 415, b'{"error":{"code":"unsupported_media_type"}}'
                    close_connection = True
                elif size > 2 * 1024 * 1024:
                    Handler.oversized_seen = True
                    status, body = 413, b'{"error":{"code":"payload_too_large"}}'
                else:
                    try:
                        valid_login = json.loads(payload) == {
                            "username": "demo", "password": "correct-horse-battery-staple"
                        }
                    except ValueError:
                        valid_login = False
                    if valid_login:
                        status, body = 200, b'{"access_token":"test-token"}'
                    else:
                        status, body = 422, b'{"error":{"code":"validation_error"}}'
                self.send_response(status)
                if close_connection:
                    self.close_connection = True
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                self.assertEqual(contract_checks("auth", server.server_port), [])
                self.assertEqual(body_limit_probe(server.server_port)["status"], 413)
                self.assertTrue(Handler.oversized_seen)
            finally:
                server.shutdown()
                thread.join(timeout=2)

    def test_auth_contract_checks_reject_wrong_error_body_and_missing_bearer_challenge(self):
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def reply(self, status, body):
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                self.reply(401, b'{"error":{"code":"unauthorized"}}')

            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                self.reply(422, b'{"error":{"code":"validation_error"}}')

        with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                errors = contract_checks("auth", server.server_port)
                self.assertIn("missing token: expected WWW-Authenticate: Bearer", errors)
                self.assertIn("missing token: unauthorized response body differs", errors)
            finally:
                server.shutdown()
                thread.join(timeout=2)

    def test_auth_contract_checks_require_case_insensitive_bearer_scheme(self):
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def reply(self, status, body, challenge=False):
                self.send_response(status)
                if challenge:
                    self.send_header("WWW-Authenticate", "Bearer")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                self.reply(401, b'{"error":{"code":"unauthorized","message":"authentication was rejected"}}', True)

            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                if self.headers.get("Content-Type") != "application/json":
                    self.reply(415, b'{"error":{"code":"unsupported_media_type"}}')
                else:
                    try:
                        valid_login = json.loads(body) == {
                            "username": "demo", "password": "correct-horse-battery-staple"
                        }
                    except ValueError:
                        valid_login = False
                    if valid_login:
                        self.reply(200, b'{"access_token":"test-token"}')
                    else:
                        self.reply(422, b'{"error":{"code":"validation_error"}}')

        with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                errors = contract_checks("auth", server.server_port)
                self.assertTrue(any("lowercase bearer scheme" in error for error in errors))
            finally:
                server.shutdown()
                thread.join(timeout=2)

    def test_posts_contract_checks_reject_wrong_not_found_and_validation_codes(self):
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def reply(self, status, body):
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                self.reply(404, b'{"error":{"code":"wrong"}}')

            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                if self.headers.get("Content-Type") == "text/plain":
                    self.reply(415, b'{"error":{"code":"wrong"}}')
                else:
                    self.reply(422, b'{"error":{"code":"wrong"}}')

        with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                errors = contract_checks("posts", server.server_port)
                self.assertEqual(len(errors), 3)
                self.assertTrue(all("response body did not match" in error for error in errors))
            finally:
                server.shutdown()
                thread.join(timeout=2)

    def test_posts_measurement_rejects_wrong_read_update_and_missing_error_content(self):
        class Handler(BaseHTTPRequestHandler):
            get_count = 0

            def log_message(self, *_args):
                pass

            def reply(self, status, body=b""):
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                self.reply(201, b'{"id":1,"title":"bench-0-0","body":"before"}')

            def do_GET(self):
                Handler.get_count += 1
                if Handler.get_count == 1:
                    self.reply(200, b'{"id":1,"title":"wrong","body":"before"}')
                else:
                    self.reply(404, b'{"error":{"code":"wrong"}}')

            def do_PUT(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                self.reply(200, b'{"id":1,"title":"bench-0-0","body":"before"}')

            def do_DELETE(self):
                self.reply(204)

        with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                result = measure("posts", server.server_port, 1, 1)
                self.assertFalse(result["passed"])
                self.assertEqual(result["error_count"], 3)
                self.assertIn("confirm deletion: response body did not match the expected contract", result["error_examples"])
            finally:
                server.shutdown()
                thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
