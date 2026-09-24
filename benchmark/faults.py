"""Loopback-only database fault injection used by the benchmark runner."""

from __future__ import annotations

import select
import socket
import subprocess
import threading
import time


class PostgresTableLock:
    """Hold an exclusive posts-table lock until the benchmark releases it."""

    def __init__(self, database_url: str) -> None:
        self.database_url = database_url
        self.process: subprocess.Popen[str] | None = None

    def __enter__(self) -> PostgresTableLock:
        self.process = subprocess.Popen(
            ["psql", "-XqAt", "-v", "ON_ERROR_STOP=1", self.database_url],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, bufsize=1,
        )
        try:
            assert self.process.stdin is not None
            assert self.process.stdout is not None
            self.process.stdin.write(
                "BEGIN;\nLOCK TABLE posts IN ACCESS EXCLUSIVE MODE;\n"
                "SELECT 'mads-lock-ready';\n"
            )
            self.process.stdin.flush()
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and self.process.poll() is None:
                if select.select([self.process.stdout], [], [], 0.1)[0]:
                    if self.process.stdout.readline().strip() == "mads-lock-ready":
                        return self
                    break
            raise RuntimeError("could not acquire the benchmark posts-table lock")
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, _exc_type: object, _exc: object, _traceback: object) -> None:
        if self.process is None:
            return
        if self.process.poll() is None and self.process.stdin is not None:
            try:
                self.process.stdin.write("ROLLBACK;\n\\q\n")
                self.process.stdin.flush()
                self.process.wait(timeout=2)
            except (OSError, subprocess.TimeoutExpired):
                self.process.terminate()
                try:
                    self.process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=2)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            if stream is not None:
                stream.close()


class StallablePostgresProxy:
    """Forward TCP traffic, with a switch to pause database-to-app replies."""

    def __init__(self, host: str, port: int) -> None:
        self.host = host
        self.target_port = port
        self.downstream_blocked = threading.Event()
        self._pause = threading.Event()
        self._stop = threading.Event()
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen()
        self._listener.settimeout(0.1)
        self.port = self._listener.getsockname()[1]
        self._accept_thread = threading.Thread(target=self._accept, daemon=True)
        self._sessions: list[threading.Thread] = []
        self._sockets: list[socket.socket] = []

    @property
    def accepted_connections(self) -> int:
        return len(self._sessions)

    def __enter__(self) -> StallablePostgresProxy:
        self._accept_thread.start()
        return self

    def __exit__(self, _exc_type: object, _exc: object, _traceback: object) -> None:
        self._stop.set()
        self._pause.clear()
        self._listener.close()
        self._accept_thread.join(timeout=1)
        for connection in self._sockets:
            connection.close()
        for session in self._sessions:
            session.join(timeout=1)

    def pause_downstream(self) -> None:
        self.downstream_blocked.clear()
        self._pause.set()

    def resume_downstream(self) -> None:
        self._pause.clear()

    def _accept(self) -> None:
        while not self._stop.is_set():
            try:
                client, _address = self._listener.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            try:
                database = socket.create_connection((self.host, self.target_port), timeout=2)
            except OSError:
                client.close()
                continue
            self._sockets.extend((client, database))
            session = threading.Thread(target=self._relay, args=(client, database), daemon=True)
            self._sessions.append(session)
            session.start()

    def _relay(self, client: socket.socket, database: socket.socket) -> None:
        while not self._stop.is_set():
            try:
                if self._pause.is_set() and select.select([database], [], [], 0)[0]:
                    self.downstream_blocked.set()
                readers = [client] if self._pause.is_set() else [client, database]
                ready, _, _ = select.select(readers, [], [], 0.05)
                for source in ready:
                    if source is database and self._pause.is_set():
                        self.downstream_blocked.set()
                        continue
                    payload = source.recv(65536)
                    if not payload:
                        return
                    (database if source is client else client).sendall(payload)
            except (OSError, ValueError):
                return
