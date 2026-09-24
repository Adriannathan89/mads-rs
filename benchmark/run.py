#!/usr/bin/env python3
"""Repeatable HTTP load and correctness checks for the MADS 0.9 examples."""

from __future__ import annotations

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from dataclasses import dataclass, field
from datetime import datetime, timezone
import http.client
import json
import math
import os
from pathlib import Path
import platform
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from typing import Callable
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from faults import PostgresTableLock, StallablePostgresProxy


ROOT = Path(__file__).resolve().parents[1]
DEMO_USERNAME = "demo"
DEMO_PASSWORD = "correct-horse-battery-staple"
DEMO_SECRET = "benchmark-only-signing-key-change-me-2026"

PROJECTS = {
    "hello": ("hello-world", "mads-example-hello-world", 3000, "/", 200),
    "auth": ("protected-route", "mads-example-protected-route", 3002, "/auth/me", 401),
    "posts": ("posts-crud", "mads-example-posts-crud", 3001, "/posts", 200),
}

PROFILES = {
    "smoke": {
        "hello": (200, 8), "connection-churn": (40, 8),
        "validation": (90, 8), "oversized": (4, 2),
        "body-limit-boundary": (12, 4), "aborted-upload": (16, 4),
        "jwt": (90, 8), "posts": (20, 4),
    },
    "stress": {
        "hello": (12_000, 64), "connection-churn": (2_000, 64),
        "validation": (3_000, 32), "oversized": (32, 8),
        "body-limit-boundary": (96, 8), "aborted-upload": (256, 32),
        "jwt": (6_000, 64), "posts": (800, 32),
    },
    "extended": {
        "hello": (50_000, 64), "connection-churn": (5_000, 64),
        "validation": (10_000, 32), "oversized": (64, 8),
        "body-limit-boundary": (192, 16), "aborted-upload": (512, 64),
        "jwt": (25_000, 64), "posts": (2_000, 32),
    },
}


def percentiles(latencies_ms: list[float]) -> dict[str, float | None]:
    """Return nearest-rank percentiles without hiding slow tail requests."""
    if not latencies_ms:
        return {"p50_ms": None, "p95_ms": None, "p99_ms": None}
    ordered = sorted(latencies_ms)
    return {
        f"p{percentile}_ms": round(ordered[math.ceil(percentile / 100 * len(ordered)) - 1], 3)
        for percentile in (50, 95, 99)
    }


def work_chunks(requests: int, concurrency: int) -> list[int]:
    """Distribute all requested operations, omitting empty workers."""
    if requests < 1 or concurrency < 1:
        raise ValueError("requests and concurrency must both be positive")
    workers = min(requests, concurrency)
    base, extra = divmod(requests, workers)
    return [base + (index < extra) for index in range(workers)]


@dataclass
class Response:
    status: int
    body: bytes
    headers: dict[str, str]
    latency_ms: float


@dataclass
class Measurements:
    latencies_ms: list[float] = field(default_factory=list)
    statuses: Counter[int] = field(default_factory=Counter)
    error_count: int = 0
    error_examples: list[str] = field(default_factory=list)

    def error(self, message: str) -> None:
        self.error_count += 1
        if len(self.error_examples) < 10:
            self.error_examples.append(message)

    def check(
        self,
        label: str,
        response: Response | None,
        expected_status: int,
        body_check: Callable[[bytes], bool] | None = None,
    ) -> bool:
        if response is None:
            return False
        self.latencies_ms.append(response.latency_ms)
        self.statuses[response.status] += 1
        if response.status != expected_status:
            self.error(f"{label}: expected HTTP {expected_status}, received {response.status}")
            return False
        try:
            valid = body_check is None or body_check(response.body)
        except (ValueError, KeyError, TypeError, UnicodeDecodeError):
            valid = False
        if not valid:
            self.error(f"{label}: response body did not match the expected contract")
        return valid

    def merge(self, other: Measurements) -> None:
        self.latencies_ms.extend(other.latencies_ms)
        self.statuses.update(other.statuses)
        self.error_count += other.error_count
        self.error_examples.extend(other.error_examples[: max(0, 10 - len(self.error_examples))])


class Client:
    def __init__(self, port: int):
        self.port = port
        self.connection = self._connect()

    def _connect(self) -> http.client.HTTPConnection:
        return http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)

    def reset(self) -> None:
        self.connection.close()
        self.connection = self._connect()

    def request(
        self,
        stats: Measurements,
        label: str,
        method: str,
        path: str,
        body: bytes | None = None,
        headers: dict[str, str] | None = None,
    ) -> Response | None:
        start = time.perf_counter_ns()
        try:
            self.connection.request(method, path, body=body, headers=headers or {})
            reply = self.connection.getresponse()
            payload = reply.read()
            elapsed_ms = (time.perf_counter_ns() - start) / 1_000_000
            return Response(
                reply.status,
                payload,
                {name.lower(): value for name, value in reply.getheaders()},
                elapsed_ms,
            )
        except (OSError, http.client.HTTPException) as error:
            stats.error(f"{label}: transport failure: {type(error).__name__}: {error}")
            self.reset()
            return None

    def close(self) -> None:
        self.connection.close()


class ManagedServer:
    def __init__(self, project: str, binary_profile: str, environment: dict[str, str]):
        directory, binary_name, port, ready_path, ready_status = PROJECTS[project]
        self.directory = ROOT / "example" / directory
        self.binary = self.directory / "target" / binary_profile / binary_name
        self.port = port
        self.ready_path = ready_path
        self.ready_status = ready_status
        self.environment = environment
        self.process: subprocess.Popen[bytes] | None = None
        self.log_file: tempfile.NamedTemporaryFile | None = None
        self.keep_log = False

    def __enter__(self) -> ManagedServer:
        if not self.binary.is_file():
            raise RuntimeError(f"missing binary {self.binary}; build it as described in benchmark/README.md")
        if port_is_open(self.port):
            raise RuntimeError(f"port {self.port} is already in use; stop that server before benchmarking")
        self.log_file = tempfile.NamedTemporaryFile(prefix="mads-benchmark-", suffix=".log", delete=False)
        self.process = subprocess.Popen(
            [str(self.binary)],
            cwd=self.directory,
            env=self.environment,
            stdout=self.log_file,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                self.log_file.close()
                raise RuntimeError(f"{self.binary.name} exited at startup; log: {self.log_file.name}")
            probe = Client(self.port)
            try:
                response = probe.request(Measurements(), "readiness", "GET", self.ready_path)
                if response is not None and response.status == self.ready_status:
                    return self
            finally:
                probe.close()
            time.sleep(0.1)
        self.__exit__(RuntimeError, None, None)
        raise RuntimeError(f"{self.binary.name} did not become ready; log: {self.log_file.name}")

    def __exit__(self, _exc_type: object, _exc: object, _traceback: object) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.send_signal(signal.SIGINT)
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=5)
        if self.log_file is not None:
            self.log_file.close()
            # Retain logs only for failing runs; successful runs leave no files.
            if _exc_type is None and not self.keep_log:
                Path(self.log_file.name).unlink(missing_ok=True)


def json_body(body: bytes) -> object:
    return json.loads(body)


def json_body_at_size(size: int) -> bytes:
    """Build a valid login JSON body with an exact byte length."""
    prefix = b'{"username":"'
    suffix = b'","password":"short"}'
    if size <= len(prefix) + len(suffix):
        raise ValueError("body size must leave room for a username")
    return prefix + b"x" * (size - len(prefix) - len(suffix)) + suffix


def statement_timeout_url(url: str, timeout_ms: int) -> str:
    """Apply PostgreSQL statement_timeout to this benchmark connection only."""
    parsed = urlsplit(url)
    query = parse_qsl(parsed.query, keep_blank_values=True)
    if any(key == "options" for key, _value in query):
        raise ValueError("database URL already has PostgreSQL options")
    query.append(("options", f"-c statement_timeout={timeout_ms}"))
    return urlunsplit(parsed._replace(query=urlencode(query)))


def proxy_database_url(url: str, port: int) -> str:
    """Route a PostgreSQL URL through a loopback TCP proxy without changing credentials."""
    parsed = urlsplit(url)
    if parsed.scheme not in ("postgres", "postgresql") or not parsed.hostname:
        raise ValueError("BENCH_DATABASE_URL must use a TCP PostgreSQL host")
    credentials, separator, _host = parsed.netloc.rpartition("@")
    authority = f"{credentials}@" if separator else ""
    return urlunsplit(parsed._replace(netloc=f"{authority}127.0.0.1:{port}"))


def port_is_open(port: int) -> bool:
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.2):
            return True
    except OSError:
        return False


def json_error(code: str) -> Callable[[bytes], bool]:
    return lambda body: json_body(body)["error"]["code"] == code


def run_workers(
    name: str,
    operations: int,
    concurrency: int,
    port: int,
    operation: Callable[[Client, Measurements, int, int], None],
) -> dict[str, object]:
    chunks = work_chunks(operations, concurrency)

    def worker(worker_id: int, count: int) -> Measurements:
        stats = Measurements()
        client = Client(port)
        try:
            for index in range(count):
                operation(client, stats, worker_id, index)
        finally:
            client.close()
        return stats

    start = time.perf_counter()
    with ThreadPoolExecutor(max_workers=len(chunks)) as executor:
        futures = [executor.submit(worker, worker_id, count) for worker_id, count in enumerate(chunks)]
        combined = Measurements()
        for future in futures:
            combined.merge(future.result())
    duration = time.perf_counter() - start
    return {
        "case": name,
        "operations": operations,
        "concurrency": len(chunks),
        "http_responses": sum(combined.statuses.values()),
        "status_counts": {str(status): count for status, count in sorted(combined.statuses.items())},
        "duration_seconds": round(duration, 3),
        "responses_per_second": round(sum(combined.statuses.values()) / duration, 1),
        **percentiles(combined.latencies_ms),
        "error_count": combined.error_count,
        "error_examples": combined.error_examples,
        "passed": combined.error_count == 0,
    }


def hello_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["hello"]

    def operation(client: Client, stats: Measurements, _worker: int, _index: int) -> None:
        response = client.request(stats, "hello", "GET", "/")
        stats.check("hello", response, 200, lambda body: body == b"Hello, world!")

    return run_workers("hello", operations, concurrency, 3000, operation)


def connection_churn_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["connection-churn"]

    def operation(client: Client, stats: Measurements, _worker: int, _index: int) -> None:
        client.reset()
        response = client.request(stats, "connection churn", "GET", "/")
        stats.check("connection churn", response, 200, lambda body: body == b"Hello, world!")

    return run_workers("connection-churn", operations, concurrency, 3000, operation)


def validation_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["validation"]
    large_input = json.dumps({"username": "x" * (128 * 1024), "password": "short"}).encode()
    invalid_input = b'{"username":"","password":"short"}'
    malformed_input = b"{"
    headers = {"Content-Type": "application/json"}

    def operation(client: Client, stats: Measurements, _worker: int, index: int) -> None:
        variant = index % 3
        label, body, status = (
            ("invalid fields", invalid_input, 422),
            ("malformed JSON", malformed_input, 422),
            ("large invalid body", large_input, 422),
        )[variant]
        response = client.request(stats, label, "POST", "/auth/login", body, headers)
        stats.check(label, response, status, json_error("validation_error"))

    return run_workers("validation", operations, concurrency, 3002, operation)


def jwt_case(profile: str) -> dict[str, object]:
    setup = Measurements()
    client = Client(3002)
    try:
        response = client.request(
            setup,
            "token setup",
            "POST",
            "/auth/login",
            json.dumps({"username": DEMO_USERNAME, "password": DEMO_PASSWORD}).encode(),
            {"Content-Type": "application/json"},
        )
        if response is None or response.status != 200:
            raise RuntimeError("could not obtain a JWT for the benchmark")
        token = json_body(response.body)["access_token"]
    finally:
        client.close()

    operations, concurrency = PROFILES[profile]["jwt"]

    def operation(client: Client, stats: Measurements, _worker: int, index: int) -> None:
        variant = index % 3
        if variant == 0:
            label, headers, status = "valid JWT", {"Authorization": f"Bearer {token}"}, 200
            check = lambda body: json_body(body) == {"id": 1, "username": DEMO_USERNAME}
        elif variant == 1:
            label, headers, status = "missing JWT", {}, 401
            check = json_error("unauthorized")
        else:
            label, headers, status = "malformed JWT", {"Authorization": "Bearer bad.token.value"}, 401
            check = json_error("unauthorized")
        response = client.request(stats, label, "GET", "/auth/me", headers=headers)
        if stats.check(label, response, status, check) and status == 401:
            if response is not None and response.headers.get("www-authenticate") != "Bearer":
                stats.error(f"{label}: missing Bearer challenge")

    return run_workers("jwt", operations, concurrency, 3002, operation)


def oversized_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["oversized"]
    large_input = json.dumps({"username": "x" * (3 * 1024 * 1024), "password": "short"}).encode()

    def operation(client: Client, stats: Measurements, _worker: int, _index: int) -> None:
        # A rejected oversized body can leave the HTTP/1.1 connection closed.
        client.reset()
        response = client.request(
            stats, "oversized JSON", "POST", "/auth/login", large_input,
            {"Content-Type": "application/json"},
        )
        stats.check("oversized JSON", response, 413, json_error("payload_too_large"))

    return run_workers("oversized", operations, concurrency, 3002, operation)


def oversized_reuse_case() -> dict[str, object]:
    """Verify clients can send another request after a signaled 413 closure."""
    large_input = json.dumps({"username": "x" * (3 * 1024 * 1024), "password": "short"}).encode()

    def operation(client: Client, stats: Measurements, _worker: int, _index: int) -> None:
        response = client.request(
            stats, "oversized connection reuse", "POST", "/auth/login", large_input,
            {"Content-Type": "application/json"},
        )
        if stats.check("oversized connection reuse", response, 413, json_error("payload_too_large")):
            if response is not None and response.headers.get("connection", "").lower() != "close":
                stats.error("413 response closed the connection without Connection: close")

    return run_workers("oversized-reuse", 8, 4, 3002, operation)


def body_limit_boundary_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["body-limit-boundary"]
    sizes = (2_097_151, 2_097_152, 2_097_153)
    payloads = [json_body_at_size(size) for size in sizes]

    def operation(client: Client, stats: Measurements, worker: int, index: int) -> None:
        variant = (worker + index) % 3
        status = 413 if variant == 2 else 422
        code = "payload_too_large" if status == 413 else "validation_error"
        response = client.request(
            stats, f"body limit {sizes[variant]}", "POST", "/auth/login", payloads[variant],
            {"Content-Type": "application/json"},
        )
        if stats.check("body limit", response, status, json_error(code)) and status == 413:
            if response is not None and response.headers.get("connection", "").lower() != "close":
                stats.error("body limit: 413 did not signal Connection: close")
        recovery = client.request(stats, "body limit recovery", "GET", "/auth/me")
        stats.check("body limit recovery", recovery, 401, json_error("unauthorized"))

    return run_workers("body-limit-boundary", operations, concurrency, 3002, operation)


def aborted_upload_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["aborted-upload"]
    login_body = json.dumps({"username": DEMO_USERNAME, "password": DEMO_PASSWORD}).encode()
    incomplete_request = (
        b"POST /auth/login HTTP/1.1\r\n"
        b"Host: 127.0.0.1\r\n"
        b"Content-Type: application/json\r\n"
        b"Content-Length: 1048576\r\n\r\n"
        + b"x" * 1024
    )

    def operation(client: Client, stats: Measurements, _worker: int, _index: int) -> None:
        try:
            with socket.create_connection(("127.0.0.1", 3002), timeout=10) as connection:
                connection.sendall(incomplete_request)
        except OSError as error:
            stats.error(f"aborted upload: transport failure: {type(error).__name__}: {error}")
        login = client.request(
            stats, "aborted upload recovery login", "POST", "/auth/login", login_body,
            {"Content-Type": "application/json"},
        )
        if not stats.check("aborted upload recovery login", login, 200):
            return
        try:
            token = json_body(login.body)["access_token"]
            if not isinstance(token, str) or not token:
                raise ValueError("missing access token")
        except (ValueError, KeyError, TypeError) as error:
            stats.error(f"aborted upload recovery login: invalid token response: {error}")
            return
        profile_response = client.request(
            stats, "aborted upload recovery profile", "GET", "/auth/me",
            headers={"Authorization": f"Bearer {token}"},
        )
        stats.check(
            "aborted upload recovery profile", profile_response, 200,
            lambda body: json_body(body) == {"id": 1, "username": DEMO_USERNAME},
        )

    return run_workers("aborted-upload", operations, concurrency, 3002, operation)


def posts_case(profile: str) -> dict[str, object]:
    operations, concurrency = PROFILES[profile]["posts"]
    headers = {"Content-Type": "application/json"}

    def operation(client: Client, stats: Measurements, worker: int, index: int) -> None:
        title = f"bench-{worker}-{index}"
        create = client.request(
            stats, "create post", "POST", "/posts",
            json.dumps({"title": title, "body": "before"}).encode(), headers,
        )
        if not stats.check("create post", create, 201):
            return
        try:
            created = json_body(create.body)
            post_id = created["id"]
            if created["title"] != title or created["body"] != "before":
                raise ValueError("created post content mismatch")
        except (ValueError, KeyError, TypeError) as error:
            stats.error(f"create post: invalid response: {error}")
            return

        path = f"/posts/{post_id}"
        read = client.request(stats, "read post", "GET", path)
        stats.check("read post", read, 200, lambda body: json_body(body)["title"] == title)
        update = client.request(
            stats, "update post", "PUT", path,
            json.dumps({"title": title, "body": "after"}).encode(), headers,
        )
        stats.check("update post", update, 200, lambda body: json_body(body)["body"] == "after")
        deleted = client.request(stats, "delete post", "DELETE", path)
        stats.check("delete post", deleted, 204)
        missing = client.request(stats, "deleted post", "GET", path)
        stats.check("deleted post", missing, 404, json_error("not_found"))

    return run_workers("posts", operations, concurrency, 3001, operation)


def database_query_timeout_recovery_case(binary_profile: str, database_url: str) -> dict[str, object]:
    """A timed-out SQL statement must not stop HTTP or poison later queries."""
    environment = os.environ.copy()
    environment["DATABASE_URL"] = statement_timeout_url(database_url, 2000)
    stats = Measurements()
    start = time.perf_counter()
    with ManagedServer("posts", binary_profile, environment) as server:
        with closing(Client(3001)) as client:
            baseline = client.request(stats, "query timeout baseline", "GET", "/posts")
            stats.check("query timeout baseline", baseline, 200)
            timeout = None
            with PostgresTableLock(database_url):
                timeout = client.request(stats, "query timeout", "GET", "/posts")
                stats.check("query timeout", timeout, 500, json_error("internal"))
                if timeout is not None and not 1500 <= timeout.latency_ms <= 5000:
                    stats.error("query did not finish near the configured 2-second timeout")
                liveness = client.request(stats, "HTTP during query timeout", "GET", "/not-a-route")
                stats.check("HTTP during query timeout", liveness, 404)
                if server.process.poll() is not None:
                    stats.error("HTTP server exited after the query timeout")
            recovered = client.request(stats, "query timeout recovery", "GET", "/posts")
            stats.check("query timeout recovery", recovered, 200)
            if server.process.poll() is not None:
                stats.error("HTTP server exited before query recovery")
        result = {
            "case": "database-query-timeout-recovery",
            "duration_seconds": round(time.perf_counter() - start, 3),
            "timeout_response_ms": None if timeout is None else round(timeout.latency_ms, 3),
            "status_counts": {str(status): count for status, count in sorted(stats.statuses.items())},
            "error_count": stats.error_count,
            "error_examples": stats.error_examples,
            "passed": stats.error_count == 0,
        }
        if not result["passed"]:
            server.keep_log = True
            result["server_log"] = server.log_file.name
        return result


def database_tcp_stall_recovery_case(binary_profile: str, database_url: str) -> dict[str, object]:
    """A stalled DB reply must not stop HTTP; a pool timeout must recover."""
    parsed = urlsplit(database_url)
    if not parsed.hostname:
        raise ValueError("BENCH_DATABASE_URL must use a TCP PostgreSQL host")
    with StallablePostgresProxy(parsed.hostname, parsed.port or 5432) as proxy:
        environment = os.environ.copy()
        environment["DATABASE_URL"] = proxy_database_url(database_url, proxy.port)
        environment["MADS_PERSISTENCE__SEAORM__MIN_CONNECTIONS"] = "1"
        environment["MADS_PERSISTENCE__SEAORM__MAX_CONNECTIONS"] = "1"
        environment["MADS_PERSISTENCE__SEAORM__ACQUIRE_TIMEOUT_SECONDS"] = "2"
        stats = Measurements()
        start = time.perf_counter()
        with ManagedServer("posts", binary_profile, environment) as server:
            startup_connections = proxy.accepted_connections
            with closing(Client(3001)) as client:
                baseline = client.request(stats, "TCP stall baseline", "GET", "/posts")
                stats.check("TCP stall baseline", baseline, 200)
                def held_query() -> tuple[Response | None, Measurements]:
                    held_stats = Measurements()
                    connection = Client(3001)
                    try:
                        response = connection.request(held_stats, "stalled connection request", "GET", "/posts")
                        return response, held_stats
                    finally:
                        connection.close()

                with ThreadPoolExecutor(max_workers=1) as executor:
                    proxy.pause_downstream()
                    timeout = None
                    held_response = None
                    try:
                        future = executor.submit(held_query)
                        if not proxy.downstream_blocked.wait(timeout=3):
                            stats.error("proxy did not observe a stalled database reply")
                        timeout = client.request(stats, "pool acquire timeout", "GET", "/posts")
                        stats.check("pool acquire timeout", timeout, 500, json_error("internal"))
                        if timeout is not None and not 1500 <= timeout.latency_ms <= 5000:
                            stats.error("pool acquire did not finish near the configured 2-second timeout")
                        liveness = client.request(stats, "HTTP during TCP stall", "GET", "/not-a-route")
                        stats.check("HTTP during TCP stall", liveness, 404)
                        if server.process.poll() is not None:
                            stats.error("HTTP server exited during the TCP stall")
                    finally:
                        proxy.resume_downstream()
                    try:
                        held_response, held_stats = future.result(timeout=5)
                        stats.merge(held_stats)
                        stats.check("stalled connection request", held_response, 500, json_error("internal"))
                        if held_response is not None and not 1500 <= held_response.latency_ms <= 5000:
                            stats.error("stalled connection did not finish near the 2-second acquire timeout")
                    except TimeoutError:
                        stats.error("stalled connection request did not finish after TCP recovery")
                recovered = client.request(stats, "TCP stall recovery", "GET", "/posts")
                stats.check("TCP stall recovery", recovered, 200)
                if server.process.poll() is not None:
                    stats.error("HTTP server exited before TCP recovery")
            result = {
                "case": "database-tcp-stall-recovery",
                "duration_seconds": round(time.perf_counter() - start, 3),
                "proxy_connections_at_startup": startup_connections,
                "proxy_connections_after_recovery": proxy.accepted_connections,
                "stalled_response_ms": None if held_response is None else round(held_response.latency_ms, 3),
                "acquire_timeout_response_ms": None if timeout is None else round(timeout.latency_ms, 3),
                "status_counts": {str(status): count for status, count in sorted(stats.statuses.items())},
                "error_count": stats.error_count,
                "error_examples": stats.error_examples,
                "passed": stats.error_count == 0,
            }
            if not result["passed"]:
                server.keep_log = True
                result["server_log"] = server.log_file.name
            return result


class StalledPostgres:
    """Accept PostgreSQL TCP connections but never complete the handshake."""

    def __init__(self) -> None:
        self.accepted = threading.Event()
        self._stop = threading.Event()
        self._connections: list[socket.socket] = []
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen()
        self._listener.settimeout(0.1)
        self.port = self._listener.getsockname()[1]
        self._thread = threading.Thread(target=self._accept, daemon=True)

    def _accept(self) -> None:
        while not self._stop.is_set():
            try:
                connection, _address = self._listener.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self._connections.append(connection)
            self.accepted.set()

    def __enter__(self) -> StalledPostgres:
        self._thread.start()
        return self

    def __exit__(self, _exc_type: object, _exc: object, _traceback: object) -> None:
        self._stop.set()
        self._listener.close()
        self._thread.join(timeout=1)
        for connection in self._connections:
            connection.close()


def database_failure_case(binary_profile: str) -> dict[str, object]:
    """A missing database must fail startup and redact connection credentials."""
    directory, binary_name, port, _path, _status = PROJECTS["posts"]
    project = ROOT / "example" / directory
    binary = project / "target" / binary_profile / binary_name
    if not binary.is_file():
        raise RuntimeError(f"missing binary {binary}; build it as described in benchmark/README.md")
    if port_is_open(port):
        raise RuntimeError(f"port {port} is already in use; stop that server before benchmarking")
    secret_marker = "benchmark-secret-should-not-leak"
    environment = os.environ.copy()
    environment["DATABASE_URL"] = f"postgres://bench:{secret_marker}@127.0.0.1:1/unavailable"
    environment["MADS_PERSISTENCE__SEAORM__CONNECT_TIMEOUT_SECONDS"] = "2"
    log = tempfile.NamedTemporaryFile(prefix="mads-benchmark-fault-", suffix=".log", delete=False)
    start = time.perf_counter()
    process = subprocess.Popen(
        [str(binary)], cwd=project, env=environment, stdout=log, stderr=subprocess.STDOUT
    )
    timed_out = False
    bound_before_ready = False
    try:
        deadline = time.monotonic() + 10
        while process.poll() is None and time.monotonic() < deadline:
            bound_before_ready |= port_is_open(port)
            time.sleep(0.05)
        if process.poll() is None:
            timed_out = True
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
    finally:
        log.close()
    contents = Path(log.name).read_text(errors="replace")
    errors = []
    if timed_out:
        errors.append("database failure did not terminate within 10 seconds")
    if bound_before_ready:
        errors.append("HTTP listener bound before database readiness succeeded")
    if process.returncode == 0:
        errors.append("database failure exited successfully instead of rejecting startup")
    if "MADS140" not in contents or "kind: Connection" not in contents:
        errors.append("startup did not report a MADS persistence connection failure")
    if secret_marker in contents:
        errors.append("database credential appeared in the startup log")
    result: dict[str, object] = {
        "case": "database-failure",
        "duration_seconds": round(time.perf_counter() - start, 3),
        "exit_code": process.returncode,
        "error_count": len(errors),
        "error_examples": errors,
        "passed": not errors,
    }
    if errors:
        result["server_log"] = log.name
    else:
        Path(log.name).unlink(missing_ok=True)
    return result


def database_connect_timeout_case(binary_profile: str) -> dict[str, object]:
    """A stalled PostgreSQL handshake must honor the configured connect timeout."""
    directory, binary_name, port, _path, _status = PROJECTS["posts"]
    project = ROOT / "example" / directory
    binary = project / "target" / binary_profile / binary_name
    if not binary.is_file():
        raise RuntimeError(f"missing binary {binary}; build it as described in benchmark/README.md")
    if port_is_open(port):
        raise RuntimeError(f"port {port} is already in use; stop that server before benchmarking")
    secret_marker = "benchmark-timeout-secret-should-not-leak"
    environment = os.environ.copy()
    environment["MADS_PERSISTENCE__SEAORM__CONNECT_TIMEOUT_SECONDS"] = "2"
    with StalledPostgres() as database:
        environment["DATABASE_URL"] = (
            f"postgres://bench:{secret_marker}@127.0.0.1:{database.port}/stalled"
        )
        log = tempfile.NamedTemporaryFile(prefix="mads-benchmark-timeout-", suffix=".log", delete=False)
        start = time.perf_counter()
        process = subprocess.Popen(
            [str(binary)], cwd=project, env=environment, stdout=log, stderr=subprocess.STDOUT
        )
        timed_out = False
        bound_before_ready = False
        try:
            deadline = time.monotonic() + 10
            while process.poll() is None and time.monotonic() < deadline:
                bound_before_ready |= port_is_open(port)
                time.sleep(0.05)
            if process.poll() is None:
                timed_out = True
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        finally:
            log.close()
        duration = time.perf_counter() - start
        accepted = database.accepted.is_set()
    contents = Path(log.name).read_text(errors="replace")
    errors = []
    if not accepted:
        errors.append("database did not accept a connection")
    if duration < 1.5 or duration > 5 or timed_out:
        errors.append("database did not finish near the configured 2-second timeout")
    if bound_before_ready:
        errors.append("HTTP listener bound before database readiness succeeded")
    if process.returncode == 0:
        errors.append("database timeout exited successfully instead of rejecting startup")
    if "MADS140" not in contents or "kind: Connection" not in contents:
        errors.append("startup did not report a MADS persistence connection failure")
    if secret_marker in contents:
        errors.append("database credential appeared in the startup log")
    result: dict[str, object] = {
        "case": "database-connect-timeout",
        "duration_seconds": round(duration, 3),
        "connection_accepted": accepted,
        "exit_code": process.returncode,
        "error_count": len(errors),
        "error_examples": errors,
        "passed": not errors,
    }
    if errors:
        result["server_log"] = log.name
    else:
        Path(log.name).unlink(missing_ok=True)
    return result


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, capture_output=True, text=True
    )
    return result.stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=PROFILES, default="smoke")
    parser.add_argument(
        "--case", action="append",
        choices=(
            "hello", "connection-churn", "validation", "oversized",
            "oversized-reuse", "body-limit-boundary", "aborted-upload",
            "jwt", "posts", "database-failure", "database-connect-timeout",
            "database-query-timeout-recovery", "database-tcp-stall-recovery",
        ),
    )
    parser.add_argument("--binary-profile", choices=("debug", "release"), default="release")
    parser.add_argument("--output", type=Path, help="write JSON results to this file")
    args = parser.parse_args()

    cases = args.case or [
        "hello", "connection-churn", "validation", "oversized",
        "oversized-reuse", "body-limit-boundary", "aborted-upload",
        "jwt", "posts", "database-failure", "database-connect-timeout",
        "database-query-timeout-recovery", "database-tcp-stall-recovery",
    ]
    database_url = os.environ.get("BENCH_DATABASE_URL")
    database_cases = ("posts", "database-query-timeout-recovery", "database-tcp-stall-recovery")
    if any(case in cases for case in database_cases) and not database_url:
        parser.error("selected database workload requires BENCH_DATABASE_URL pointing to an isolated database with the posts migration applied")

    report: dict[str, object] = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "git_head": git_head(),
        "profile": args.profile,
        "binary_profile": args.binary_profile,
        "platform": platform.platform(),
        "python": platform.python_version(),
        "logical_cpus": os.cpu_count(),
        "cases": [],
    }
    results: list[dict[str, object]] = []
    try:
        if "hello" in cases or "connection-churn" in cases:
            with ManagedServer("hello", args.binary_profile, os.environ.copy()) as server:
                if "hello" in cases:
                    result = hello_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
                if "connection-churn" in cases:
                    result = connection_churn_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
        auth_cases = (
            "validation", "oversized", "oversized-reuse",
            "body-limit-boundary", "aborted-upload", "jwt",
        )
        if any(case in cases for case in auth_cases):
            environment = os.environ.copy()
            environment.update({
                "DEMO_USERNAME": DEMO_USERNAME,
                "DEMO_PASSWORD": DEMO_PASSWORD,
                "JWT_SECRET": DEMO_SECRET,
            })
            with ManagedServer("auth", args.binary_profile, environment) as server:
                if "validation" in cases:
                    result = validation_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
                if "oversized" in cases:
                    result = oversized_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
                if "oversized-reuse" in cases:
                    result = oversized_reuse_case()
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
                if "body-limit-boundary" in cases:
                    result = body_limit_boundary_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
                if "aborted-upload" in cases:
                    result = aborted_upload_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
                if "jwt" in cases:
                    result = jwt_case(args.profile)
                    if not result["passed"]:
                        server.keep_log = True
                        result["server_log"] = server.log_file.name
                    results.append(result)
        if "posts" in cases:
            environment = os.environ.copy()
            environment["DATABASE_URL"] = database_url
            with ManagedServer("posts", args.binary_profile, environment) as server:
                result = posts_case(args.profile)
                if not result["passed"]:
                    server.keep_log = True
                    result["server_log"] = server.log_file.name
                results.append(result)
        if "database-failure" in cases:
            results.append(database_failure_case(args.binary_profile))
        if "database-connect-timeout" in cases:
            results.append(database_connect_timeout_case(args.binary_profile))
        if "database-query-timeout-recovery" in cases:
            results.append(database_query_timeout_recovery_case(args.binary_profile, database_url))
        if "database-tcp-stall-recovery" in cases:
            results.append(database_tcp_stall_recovery_case(args.binary_profile, database_url))
    except Exception as error:
        report["fatal_error"] = f"{type(error).__name__}: {error}"
    report["cases"] = results
    report["passed"] = "fatal_error" not in report and all(result["passed"] for result in results)
    rendered = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + "\n")
    print(rendered)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
