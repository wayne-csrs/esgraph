#!/usr/bin/env python3
"""Minimal HTTP server for VM attack scenarios: serve benign payloads and capture uploads."""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import unquote


class ScenarioHTTPHandler(BaseHTTPRequestHandler):
    server: "ScenarioHTTPServer"

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write(
            f"[{datetime.now(timezone.utc).isoformat()}] {self.address_string()} {fmt % args}\n"
        )

    def do_GET(self) -> None:
        path = unquote(self.path.split("?", 1)[0])
        if path == "/frozenfix/update":
            self._serve_payload("helper.sh", "application/x-sh")
            return
        if path.startswith("/curl/"):
            self._serve_payload("stage1.sh", "application/x-sh")
            return
        if path == "/health":
            self._send_bytes(b"ok\n", "text/plain", 200)
            return
        self.send_error(404, "not found")

    def do_POST(self) -> None:
        path = unquote(self.path.split("?", 1)[0])
        if path in ("/upload", "/"):
            self._handle_upload()
            return
        self.send_error(404, "not found")

    def _payload_path(self, name: str) -> Path:
        return self.server.payload_dir / name

    def _serve_payload(self, name: str, content_type: str) -> None:
        payload = self._payload_path(name)
        if not payload.is_file():
            self.send_error(404, f"missing payload: {name}")
            return
        self._send_bytes(payload.read_bytes(), content_type, 200)

    def _send_bytes(self, body: bytes, content_type: str, status: int) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _handle_upload(self) -> None:
        length = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(length) if length else b""
        content_type = self.headers.get("Content-Type", "")

        saved: list[str] = []
        if "multipart/form-data" in content_type:
            saved.extend(self._save_multipart(raw, content_type))
        elif raw:
            stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
            out = self.server.upload_dir / f"upload-{stamp}.bin"
            out.write_bytes(raw)
            saved.append(out.name)

        meta = {
            "received_at": datetime.now(timezone.utc).isoformat(),
            "path": self.path,
            "remote": self.client_address[0],
            "content_type": content_type,
            "saved_files": saved,
            "bytes": len(raw),
        }
        self.server.upload_dir.joinpath("last-upload.json").write_text(
            json.dumps(meta, indent=2) + "\n",
            encoding="utf-8",
        )
        self._send_bytes(json.dumps(meta).encode("utf-8"), "application/json", 200)

    def _save_multipart(self, raw: bytes, content_type: str) -> list[str]:
        boundary_match = re.search(r"boundary=([^;\s]+)", content_type)
        if not boundary_match:
            return []
        boundary = boundary_match.group(1).strip().strip('"')
        delimiter = f"--{boundary}".encode("ascii")
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        saved: list[str] = []

        for chunk in raw.split(delimiter):
            chunk = chunk.strip(b"\r\n")
            if not chunk or chunk == b"--":
                continue
            header_end = chunk.find(b"\r\n\r\n")
            if header_end < 0:
                continue
            headers = chunk[:header_end].decode("latin-1", errors="replace")
            body = chunk[header_end + 4 :]
            if body.endswith(b"\r\n"):
                body = body[:-2]

            filename = ""
            match = re.search(r'filename="([^"]+)"', headers)
            if match:
                filename = Path(match.group(1)).name
            if not filename:
                name_match = re.search(r"name=\"([^\"]+)\"", headers)
                field = name_match.group(1) if name_match else "data"
                filename = f"field-{field}-{stamp}.txt"

            out = self.server.upload_dir / filename
            out.write_bytes(body)
            saved.append(out.name)
        return saved


class ScenarioHTTPServer(ThreadingHTTPServer):
    def __init__(
        self,
        server_address: tuple[str, int],
        handler_cls: type[ScenarioHTTPHandler],
        *,
        root: Path,
    ) -> None:
        super().__init__(server_address, handler_cls)
        self.root = root
        self.payload_dir = root / "payloads"
        self.upload_dir = root / "uploads"
        self.payload_dir.mkdir(parents=True, exist_ok=True)
        self.upload_dir.mkdir(parents=True, exist_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path, help="Server state directory")
    parser.add_argument("--bind", default="0.0.0.0", help="Address to bind")
    parser.add_argument("--port", type=int, default=8765, help="TCP port")
    args = parser.parse_args()

    args.root.mkdir(parents=True, exist_ok=True)
    server = ScenarioHTTPServer((args.bind, args.port), ScenarioHTTPHandler, root=args.root)
    sys.stderr.write(
        f"scenario-http-server listening on http://{args.bind}:{args.port} root={args.root}\n"
    )
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
