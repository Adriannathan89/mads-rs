#!/usr/bin/env python3
"""Run equivalent MADS, Axum, and Fiber HTTP workloads sequentially."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import signal
import statistics
import subprocess
import tempfile
import time

from run import (
    Client, Measurements, ROOT, DEMO_USERNAME, DEMO_PASSWORD, DEMO_SECRET,
    PROFILES, json_body, json_error, port_is_open, run_workers,
)


def median_summary(runs: list[dict[str, object]]) -> dict[str, object]:
    if not runs:
        raise ValueError("at least one measured run is required")
    return {
        "runs": len(runs),
        "median_responses_per_second": statistics.median(float(run["responses_per_second"]) for run in runs),
        "median_p50_ms": statistics.median(float(run["p50_ms"]) for run in runs),
        "median_p95_ms": statistics.median(float(run["p95_ms"]) for run in runs),
        "median_p99_ms": statistics.median(float(run["p99_ms"]) for run in runs),
        "passed": all(bool(run["passed"]) for run in runs),
    }


def warmup_size(operations: int, concurrency: int) -> tuple[int, int]:
    return min(operations, max(200, concurrency * 10)), concurrency


SCENARIOS = ("hello", "posts", "auth")
FRAMEWORKS = ("mads", "axum", "fiber")
PORTS = {"hello": 3000, "posts": 3001, "auth": 3002}
MADS_BINARIES = {
    "hello": ("hello-world", "mads-example-hello-world"),
    "posts": ("posts-crud", "mads-example-posts-crud"),
    "auth": ("protected-route", "mads-example-protected-route"),
}


def target_command(framework: str, scenario: str) -> tuple[list[str], Path]:
    if framework == "mads":
        directory, binary = MADS_BINARIES[scenario]
        project = ROOT / "example" / directory
        return [str(ROOT / "benchmark" / "targets" / "mads" / "target" / "release" / binary)], project
    if framework == "axum":
        project = ROOT / "benchmark" / "targets" / "axum"
        return [str(project / "target" / "release" / "mads-bench-axum")], project
    project = ROOT / "benchmark" / "targets" / "fiber"
    return [str(project / "bin" / "mads-bench-fiber")], project


@contextmanager
def server(framework: str, scenario: str, database_url: str | None):
    port = PORTS[scenario]
    command, directory = target_command(framework, scenario)
    if not Path(command[0]).is_file():
        raise RuntimeError(f"missing binary: {command[0]}")
    if port_is_open(port):
        raise RuntimeError(f"port {port} is already in use")
    environment = os.environ.copy()
    environment.update({
        "BENCH_MODE": scenario, "PORT": str(port),
        "DEMO_USERNAME": DEMO_USERNAME, "DEMO_PASSWORD": DEMO_PASSWORD,
        "JWT_SECRET": DEMO_SECRET,
    })
    if database_url is not None:
        environment["DATABASE_URL"] = database_url
    with tempfile.NamedTemporaryFile(prefix=f"bench-{framework}-{scenario}-", suffix=".log", delete=False) as log:
        log_path = Path(log.name)
        process = subprocess.Popen(command, cwd=directory, env=environment, stdout=log, stderr=subprocess.STDOUT)
        try:
            ready_path, ready_status = ("/auth/me", 401) if scenario == "auth" else (("/posts", 200) if scenario == "posts" else ("/", 200))
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"{framework}/{scenario} exited during startup; log: {log_path}")
                client = Client(port)
                try:
                    response = client.request(Measurements(), "readiness", "GET", ready_path)
                    if response is not None and response.status == ready_status:
                        break
                finally:
                    client.close()
                time.sleep(0.1)
            else:
                raise RuntimeError(f"{framework}/{scenario} was not ready; log: {log_path}")
            yield port, log_path
        except Exception:
            raise
        else:
            log_path.unlink(missing_ok=True)
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


def request_checked(client: Client, stats: Measurements, label: str, method: str, path: str,
                    status: int, body: bytes | None = None, headers: dict[str, str] | None = None,
                    body_check=None):
    response = client.request(stats, label, method, path, body, headers)
    stats.check(label, response, status, body_check)
    return response


def contract_checks(scenario: str, port: int) -> list[str]:
    errors = []
    client = Client(port)
    try:
        stats = Measurements()
        if scenario == "hello":
            response = request_checked(client, stats, "hello", "GET", "/", 200)
            if response is not None and response.body != b"Hello, world!":
                errors.append("hello response body differs")
        elif scenario == "auth":
            for label, headers in (
                ("missing token", None),
                ("malformed token", {"Authorization": "Bearer bad.token.value"}),
            ):
                response = client.request(stats, label, "GET", "/auth/me", headers=headers)
                stats.check(label, response, 401)
                if response is None or response.status != 401:
                    continue
                if response.headers.get("www-authenticate") != "Bearer":
                    stats.error(f"{label}: expected WWW-Authenticate: Bearer")
                try:
                    body = json_body(response.body)
                except (ValueError, TypeError):
                    body = None
                if body != {"error": {"code": "unauthorized", "message": "authentication was rejected"}}:
                    stats.error(f"{label}: unauthorized response body differs")
            request_checked(client, stats, "invalid input", "POST", "/auth/login", 422,
                            b'{"username":"","password":"short"}', {"Content-Type": "application/json"},
                            json_error("validation_error"))
            request_checked(client, stats, "invalid JSON", "POST", "/auth/login", 422,
                            b"{", {"Content-Type": "application/json"}, json_error("validation_error"))
            request_checked(client, stats, "unsupported content type", "POST", "/auth/login", 415,
                            b'{"username":"","password":"short"}', {"Content-Type": "text/plain"},
                            json_error("unsupported_media_type"))
            # Some servers close the connection after rejecting the request even
            # when the response does not advertise `Connection: close`.
            client.reset()
            login = request_checked(
                client, stats, "valid login", "POST", "/auth/login", 200,
                json.dumps({"username": DEMO_USERNAME, "password": DEMO_PASSWORD}).encode(),
                {"Content-Type": "application/json"},
                lambda body: isinstance(json_body(body), dict) and isinstance(json_body(body).get("access_token"), str),
            )
            if login is not None and login.status == 200:
                try:
                    token = json_body(login.body)["access_token"]
                except (ValueError, KeyError, TypeError):
                    token = None
                if isinstance(token, str) and token:
                    request_checked(
                        client, stats, "lowercase bearer scheme", "GET", "/auth/me", 200,
                        headers={"Authorization": f"bearer {token}"},
                        body_check=lambda body: json_body(body) == {"id": 1, "username": DEMO_USERNAME},
                    )
                else:
                    stats.error("valid login did not provide a usable access token")
        else:
            request_checked(client, stats, "missing post", "GET", "/posts/2147483647", 404,
                            body_check=json_error("not_found"))
            request_checked(client, stats, "invalid post", "POST", "/posts", 422,
                            b'{"title":"","body":"x"}', {"Content-Type": "application/json"},
                            json_error("validation_error"))
            request_checked(client, stats, "unsupported content type", "POST", "/posts", 415,
                            b'{"title":"","body":"x"}', {"Content-Type": "text/plain"},
                            json_error("unsupported_media_type"))
        errors.extend(stats.error_examples)
    finally:
        client.close()
    return errors


def body_limit_probe(port: int) -> dict[str, object]:
    """Observe a 3 MiB upload separately from the measured auth hot path."""
    oversized = b'{"username":"' + b"x" * (3 * 1024 * 1024) + b'","password":"short"}'
    stats = Measurements()
    client = Client(port)
    try:
        response = client.request(
            stats, "oversized body", "POST", "/auth/login", oversized,
            {"Content-Type": "application/json"},
        )
        return {"status": response.status if response is not None else None,
                "transport_errors": stats.error_examples}
    finally:
        client.close()


def measure(scenario: str, port: int, operations: int, concurrency: int) -> dict[str, object]:
    headers = {"Content-Type": "application/json"}

    def operation(client: Client, stats: Measurements, worker: int, index: int) -> None:
        if scenario == "hello":
            response = request_checked(client, stats, "hello", "GET", "/", 200)
            if response is not None and response.body != b"Hello, world!":
                stats.error("hello response body differs")
        elif scenario == "auth":
            login = request_checked(client, stats, "login", "POST", "/auth/login", 200,
                                    json.dumps({"username": DEMO_USERNAME, "password": DEMO_PASSWORD}).encode(), headers)
            if login is None or login.status != 200:
                return
            try:
                token = json_body(login.body)["access_token"]
            except (ValueError, KeyError, TypeError):
                stats.error("login did not return access_token")
                return
            profile = request_checked(client, stats, "profile", "GET", "/auth/me", 200,
                                      headers={"Authorization": f"Bearer {token}"})
            if profile is not None and profile.status == 200:
                try:
                    if json_body(profile.body) != {"id": 1, "username": DEMO_USERNAME}:
                        stats.error("profile content differs")
                except (ValueError, TypeError):
                    stats.error("profile JSON is invalid")
        else:
            title = f"bench-{worker}-{index}"
            created = request_checked(client, stats, "create", "POST", "/posts", 201,
                                      json.dumps({"title": title, "body": "before"}).encode(), headers)
            if created is None or created.status != 201:
                return
            try:
                post = json_body(created.body)
                post_id = post["id"]
                if post["title"] != title or post["body"] != "before":
                    raise ValueError("create content differs")
            except (ValueError, KeyError, TypeError) as error:
                stats.error(f"invalid create response: {error}")
                return
            path = f"/posts/{post_id}"
            read = request_checked(client, stats, "read", "GET", path, 200)
            if read is not None and read.status == 200:
                try:
                    if json_body(read.body) != {"id": post_id, "title": title, "body": "before"}:
                        stats.error("read post content differs")
                except (ValueError, TypeError):
                    stats.error("read post JSON is invalid")
            updated = request_checked(client, stats, "update", "PUT", path, 200,
                                      json.dumps({"title": title, "body": "after"}).encode(), headers)
            if updated is not None and updated.status == 200:
                try:
                    if json_body(updated.body) != {"id": post_id, "title": title, "body": "after"}:
                        stats.error("updated post content differs")
                except (ValueError, TypeError):
                    stats.error("updated post JSON is invalid")
            request_checked(client, stats, "delete", "DELETE", path, 204)
            request_checked(client, stats, "confirm deletion", "GET", path, 404,
                            body_check=json_error("not_found"))

    return run_workers(scenario, operations, concurrency, port, operation)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--framework", action="append", choices=FRAMEWORKS)
    parser.add_argument("--scenario", action="append", choices=SCENARIOS)
    parser.add_argument("--profile", choices=PROFILES, default="smoke")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    frameworks = args.framework or FRAMEWORKS
    scenarios = args.scenario or SCENARIOS
    report = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(), "machine": platform.machine(),
        "logical_cpus": os.cpu_count(), "python": platform.python_version(),
        "git_head": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip(),
        "rustc": subprocess.run(["rustc", "--version"], check=True, capture_output=True, text=True).stdout.strip(),
        "go": subprocess.run(["go", "version"], check=True, capture_output=True, text=True).stdout.strip(),
        "versions": {
            "mads": "0.9.1 (local source)", "axum": "0.8.9", "fiber": "2.52.15",
            "postgres": os.environ.get("BENCH_POSTGRES_VERSION", "not captured"),
        },
        "profile": args.profile, "measured_runs": args.runs, "results": [],
    }
    for scenario in scenarios:
        operations, concurrency = PROFILES[args.profile]["jwt" if scenario == "auth" else scenario]
        for framework in frameworks:
            item = {"framework": framework, "scenario": scenario, "runs": []}
            database_url = os.environ.get(f"BENCH_{framework.upper()}_DATABASE_URL") if scenario == "posts" else None
            try:
                if scenario == "posts" and not database_url:
                    raise RuntimeError(f"BENCH_{framework.upper()}_DATABASE_URL is required for posts")
                with server(framework, scenario, database_url) as (port, server_log):
                    item["contract_errors"] = contract_checks(scenario, port)
                    if scenario == "auth":
                        item["body_limit"] = body_limit_probe(port)
                    if item["contract_errors"]:
                        raise RuntimeError(f"contract checks failed; server log: {server_log}")
                    warmup_operations, warmup_concurrency = warmup_size(operations, concurrency)
                    warmup = measure(scenario, port, warmup_operations, warmup_concurrency)
                    if not warmup["passed"]:
                        raise RuntimeError(f"warmup failed: {warmup['error_examples']}; server log: {server_log}")
                    for _ in range(args.runs):
                        measured = measure(scenario, port, operations, concurrency)
                        item["runs"].append(measured)
                        if not measured["passed"]:
                            raise RuntimeError(f"measured run failed: {measured['error_examples']}; server log: {server_log}")
                item["summary"] = median_summary(item["runs"])
            except Exception as error:
                item["error"] = f"{type(error).__name__}: {error}"
            report["results"].append(item)
    report["passed"] = all("error" not in item and item["summary"]["passed"] for item in report["results"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
