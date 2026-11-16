"""A static file server for local development and for the browser golden check.

    python web/serve.py [--port 8731] [--result FILE]

Serves the repository root on the loopback interface. Open /web/ for the
editor page, or /web/golden.html to run the corpus through the WebAssembly
build. With --result, the verdict the golden page reports is written to FILE
and the server exits, which lets a script drive a headless browser.
"""

import argparse
import functools
import http.server
import pathlib
import sys
import threading

ROOT = pathlib.Path(__file__).resolve().parent.parent


class Handler(http.server.SimpleHTTPRequestHandler):
    result_file = None

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_POST(self):
        if self.path != "/__golden_result" or self.result_file is None:
            self.send_error(404)
            return
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        pathlib.Path(self.result_file).write_bytes(body)
        self.send_response(204)
        self.end_headers()
        threading.Thread(target=self.server.shutdown, daemon=True).start()

    def log_message(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=8731)
    parser.add_argument("--result", help="write the golden verdict here, then exit")
    args = parser.parse_args()
    Handler.result_file = args.result
    Handler.extensions_map = {**Handler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript", ".mjs": "text/javascript"}
    handler = functools.partial(Handler, directory=str(ROOT))
    with http.server.ThreadingHTTPServer(("127.0.0.1", args.port), handler) as server:
        print(f"serving {ROOT} at http://127.0.0.1:{args.port}/web/", file=sys.stderr)
        server.serve_forever()


if __name__ == "__main__":
    main()
