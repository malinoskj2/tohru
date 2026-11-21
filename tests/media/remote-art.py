#!/usr/bin/env python3
"""Verify bin/tohru-media-art URL validation, image sniffing, caching and error handling."""
import http.server
import os
from pathlib import Path
import socketserver
import subprocess
import sys
import tempfile
import threading
import unittest

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "bin/tohru-media-art"
# Minimal valid 1x1 PNG
PNG_1X1 = (
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
    b"\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\rIDATx\x9cc`\x00\x00"
    b"\x00\x02\x00\x01H\xaf\xa4q\x00\x00\x00\x00IEND\xaeB`\x82"
)


class MediaArtTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="tohru-media-art-")
        self.addCleanup(self.tmp.cleanup)
        self.hits = []
        hits = self.hits

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                hits.append(self.path)
                if self.path == "/cover.png":
                    self.send_response(200)
                    self.send_header("Content-Type", "image/png")
                    self.end_headers()
                    self.wfile.write(PNG_1X1)
                elif self.path == "/not-image":
                    self.send_response(200)
                    self.send_header("Content-Type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"<html>nope</html>")
                else:
                    self.send_response(404)
                    self.end_headers()

            def log_message(self, *args):
                pass

        self.server = socketserver.TCPServer(("127.0.0.1", 0), Handler)
        self.addCleanup(self.server.server_close)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.server.shutdown)
        self.env = {**os.environ, "TOHRU_MEDIA_ART_CACHE": self.tmp.name}

    def run_helper(self, *args):
        return subprocess.run([str(SCRIPT), *args], env=self.env, text=True, capture_output=True)

    def test_fetches_and_caches_png(self):
        url = f"http://127.0.0.1:{self.port}/cover.png"
        first = self.run_helper(url)
        self.assertEqual(first.returncode, 0, first.stderr)
        path = Path(first.stdout.strip())
        self.assertTrue(path.is_file() and path.suffix == ".png")
        self.assertEqual(path.read_bytes(), PNG_1X1)
        self.assertEqual(self.hits, ["/cover.png"])

        second = self.run_helper(url)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(second.stdout.strip(), str(path))
        self.assertEqual(self.hits, ["/cover.png"], "second call must serve from cache")

    def test_rejects_non_image_and_bad_schemes(self):
        bad_img = self.run_helper(f"http://127.0.0.1:{self.port}/not-image")
        self.assertNotEqual(bad_img.returncode, 0)
        self.assertEqual(bad_img.stdout, "")

        for bad in ("file:///etc/passwd", "ftp://127.0.0.1/a.png", "http://127.0.0.1/a\nb", ""):
            res = self.run_helper(bad)
            self.assertNotEqual(res.returncode, 0, bad)
            self.assertEqual(res.stdout, "")


if __name__ == "__main__":
    unittest.main()
