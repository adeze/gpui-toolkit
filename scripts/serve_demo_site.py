#!/usr/bin/env python3
"""Preview the built demo site with WebGPU headers and the Spinorama API proxy."""

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError, URLError
from urllib.request import urlopen


class DemoHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        super().end_headers()

    def do_GET(self):
        prefix = "/api/spinorama/v1/"
        if not self.path.startswith(prefix):
            return super().do_GET()
        url = "https://api.spinorama.org/v1/" + self.path[len(prefix):]
        try:
            with urlopen(url, timeout=60) as response:
                body = response.read()
                self.send_response(response.status)
                self.send_header("Content-Type", response.headers.get("Content-Type", "application/json"))
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
        except HTTPError as error:
            self.send_error(error.code, "Spinorama API request failed")
        except URLError:
            self.send_error(502, "Spinorama API unavailable")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", default="target/demo-site")
    parser.add_argument("--port", type=int, default=8090)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), partial(DemoHandler, directory=args.directory))
    print(f"Demo site: http://127.0.0.1:{args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
