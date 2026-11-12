#!/usr/bin/env python3
"""Failure-inject Blackbox probes and test Alertmanager delivery locally."""

from __future__ import annotations

import http.server
import json
import pathlib
import re
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


ROOT = pathlib.Path(__file__).resolve().parents[1]
MONITORING = ROOT / "docker/monitoring"
COMPOSE = (ROOT / "docker-compose.monitoring.yml").read_text()
BB_IMAGE = re.search(r"(?m)^\s+image: (prom/blackbox-exporter@sha256:[a-f0-9]{64})$", COMPOSE)
AM_IMAGE = re.search(r"(?m)^\s+image: (prom/alertmanager@sha256:[a-f0-9]{64})$", COMPOSE)
if not BB_IMAGE or not AM_IMAGE:
    raise SystemExit("Could not find digest-pinned monitoring images in Compose")


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_http(url: str, timeout: float = 25) -> bytes:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=1) as response:
                return response.read()
        except Exception as error:  # readiness polling intentionally retries
            last_error = error
            time.sleep(0.2)
    raise RuntimeError(f"HTTP endpoint did not become ready: {url}: {last_error}")


def probe_success(base: str, module: str, target: str) -> int:
    query = urllib.parse.urlencode({"module": module, "target": target})
    body = wait_http(f"{base}/probe?{query}", timeout=8).decode()
    match = re.search(r"(?m)^probe_success\s+([01])(?:\.0)?$", body)
    if not match:
        raise RuntimeError(f"probe response omitted probe_success for {module}")
    return int(match.group(1))


probe_status = {"code": 200}
receiver_status = {"code": 500}
received_webhooks: list[dict] = []
webhook_attempts = 0


class FixtureHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        self.send_response(probe_status["code"])
        self.end_headers()
        self.wfile.write(b"fixture")

    def log_message(self, *_: object) -> None:
        return


class WebhookHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        global webhook_attempts
        size = int(self.headers.get("Content-Length", "0"))
        notice = json.loads(self.rfile.read(size))
        webhook_attempts += 1
        self.send_response(receiver_status["code"])
        self.end_headers()
        if receiver_status["code"] == 200:
            received_webhooks.append(notice)

    def log_message(self, *_: object) -> None:
        return


class TcpFixture:
    def __init__(self) -> None:
        self.socket = socket.socket()
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.socket.bind(("127.0.0.1", 0))
        self.socket.listen()
        self.socket.settimeout(0.2)
        self.port = self.socket.getsockname()[1]
        self.running = True
        self.thread = threading.Thread(target=self._accept, daemon=True)
        self.thread.start()

    def _accept(self) -> None:
        while self.running:
            try:
                conn, _ = self.socket.accept()
                conn.close()
            except TimeoutError:
                continue
            except OSError:
                return

    def close(self) -> None:
        self.running = False
        self.socket.close()
        self.thread.join(timeout=1)


def container_run(name: str, image: str, *args: str, mounts: tuple[str, ...] = ()) -> None:
    command = ["docker", "run", "--rm", "-d", "--network", "host", "--name", name]
    for mount in mounts:
        command.extend(("-v", mount))
    command.append(image)
    command.extend(args)
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL)


def container_stop(name: str) -> None:
    subprocess.run(["docker", "rm", "-f", name], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def test_blackbox_failure_injection() -> None:
    dogfood = (ROOT / "docker-compose.dogfood.yml").read_text()
    if not re.search(r"(?m)^\s+buzz-relay:\s*\n\s+network_mode: service:backend", dogfood):
        raise AssertionError("expected Buzz relay to share the backend network namespace")
    if not re.search(r"(?m)^\s+chat-observer:\s*\n\s+network_mode: service:backend", dogfood):
        raise AssertionError("expected Chat observer to share the backend network namespace")
    if "http://backend:8091/ready" not in (MONITORING / "chat-observer-targets.yml").read_text():
        raise AssertionError("observer probe must target its backend namespace alias")
    if "backend:7447" not in (MONITORING / "chat-relay-targets.yml").read_text():
        raise AssertionError("relay probe must target its backend namespace alias")

    http_server = http.server.ThreadingHTTPServer(("0.0.0.0", 0), FixtureHandler)
    http_thread = threading.Thread(target=http_server.serve_forever, daemon=True)
    http_thread.start()
    tcp = TcpFixture()
    port = free_port()
    name = f"g0-blackbox-{uuid.uuid4().hex[:8]}"
    base = f"http://127.0.0.1:{port}"
    config = str(MONITORING / "blackbox.yml") + ":/etc/blackbox/blackbox.yml:ro"
    try:
        container_run(
            name,
            BB_IMAGE.group(1),
            "--config.file=/etc/blackbox/blackbox.yml",
            f"--web.listen-address=127.0.0.1:{port}",
            mounts=(config,),
        )
        wait_http(f"{base}/-/healthy")

        http_target = f"http://127.0.0.1:{http_server.server_port}/ready"
        if probe_success(base, "http_2xx", http_target) != 1:
            raise AssertionError("healthy observer-style HTTP target did not pass")
        probe_status["code"] = 503
        if probe_success(base, "http_2xx", http_target) != 0:
            raise AssertionError("unready observer-style HTTP target did not fail")
        probe_status["code"] = 200

        tcp_target = f"127.0.0.1:{tcp.port}"
        if probe_success(base, "tcp_connect", tcp_target) != 1:
            raise AssertionError("listening relay-style TCP target did not pass")
        tcp.close()
        if probe_success(base, "tcp_connect", tcp_target) != 0:
            raise AssertionError("stopped relay-style TCP target did not fail")
        print("PASS: Blackbox HTTP and TCP probes detect injected failure and recovery")
    finally:
        container_stop(name)
        tcp.close()
        http_server.shutdown()
        http_server.server_close()


def test_alertmanager_delivery() -> None:
    webhook = http.server.ThreadingHTTPServer(("0.0.0.0", 0), WebhookHandler)
    webhook_thread = threading.Thread(target=webhook.serve_forever, daemon=True)
    webhook_thread.start()
    api_port = free_port()
    name = f"g0-alertmanager-{uuid.uuid4().hex[:8]}"
    config_text = f"""global:
  resolve_timeout: 1m
route:
  receiver: isolated
  group_by: [alertname]
  group_wait: 0s
  group_interval: 1s
  repeat_interval: 1m
receivers:
  - name: isolated
    webhook_configs:
      - url: http://127.0.0.1:{webhook.server_port}/hook
        send_resolved: true
"""
    with tempfile.TemporaryDirectory(prefix="elembra-alertmanager-test-") as directory:
        config_path = pathlib.Path(directory) / "alertmanager.yml"
        config_path.write_text(config_text)
        mount = f"{config_path}:/tmp/alertmanager.yml:ro"
        base = f"http://127.0.0.1:{api_port}"
        try:
            container_run(
                name,
                AM_IMAGE.group(1),
                "--config.file=/tmp/alertmanager.yml",
                "--storage.path=/tmp/alertmanager-data",
                f"--web.listen-address=127.0.0.1:{api_port}",
                "--cluster.listen-address=",
                mounts=(mount,),
            )
            wait_http(f"{base}/-/ready")
            def send_alert(name: str) -> None:
                payload = json.dumps(
                    [
                        {
                            "labels": {"alertname": name, "severity": "warning"},
                            "annotations": {"summary": "isolated delivery test"},
                            "startsAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                        }
                    ]
                ).encode()
                request = urllib.request.Request(
                    f"{base}/api/v2/alerts", data=payload,
                    headers={"Content-Type": "application/json"},
                )
                with urllib.request.urlopen(request, timeout=3) as response:
                    if response.status not in (200, 202):
                        raise AssertionError(f"Alertmanager rejected isolated alert: {response.status}")

            send_alert("ElembraIsolatedDeliveryFailureTest")
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                metrics = wait_http(f"{base}/metrics", timeout=3).decode()
                failures = re.findall(
                    r"(?m)^alertmanager_notifications_failed_total(?:\{[^}]*\})?\s+([0-9.]+)$",
                    metrics,
                )
                if any(float(value) > 0 for value in failures):
                    break
                time.sleep(0.2)
            else:
                raise AssertionError("failed isolated delivery was not reflected in Alertmanager metrics")

            receiver_status["code"] = 200
            send_alert("ElembraIsolatedDeliveryTest")
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline and not received_webhooks:
                time.sleep(0.2)
            if not received_webhooks:
                raise AssertionError("isolated webhook receiver did not receive a successful alert")
            names = {
                alert.get("labels", {}).get("alertname")
                for notice in received_webhooks
                for alert in notice.get("alerts", [])
            }
            if "ElembraIsolatedDeliveryTest" not in names:
                raise AssertionError("webhook payload did not contain the injected alert")
            print("PASS: Alertmanager detects receiver failure and delivers after recovery")
        finally:
            container_stop(name)
            webhook.shutdown()
            webhook.server_close()


if __name__ == "__main__":
    test_blackbox_failure_injection()
    test_alertmanager_delivery()
