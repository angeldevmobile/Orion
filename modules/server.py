"""
Orion Server Module — Fase 5A
Servidor HTTP con routing declarativo. Sin dependencias externas.
"""
import http.server
import json as _json
import re
import urllib.parse


# ---------------------------------------------------------------------------
# Request object
# ---------------------------------------------------------------------------

class _OrionRequest:
    """Objeto request pasado a los handlers de rutas."""

    __slots__ = ("path", "method", "body", "params", "url_params", "headers")

    def __init__(self, method, path, body, params, url_params, headers):
        self.method     = method
        self.path       = path
        self.body       = body
        self.params     = params       # query string params
        self.url_params = url_params   # path params  (:id)
        self.headers    = headers

    def json(self):
        """Parsea el body como JSON y retorna un dict."""
        try:
            return _json.loads(self.body) if self.body else {}
        except _json.JSONDecodeError:
            return {}

    # Acceso por clave para compatibilidad con dict
    def __getitem__(self, key):
        return getattr(self, key, self.params.get(key))

    def get(self, key, default=None):
        val = getattr(self, key, None)
        return val if val is not None else default

    def __repr__(self):
        return f"<Request {self.method} {self.path}>"


# ---------------------------------------------------------------------------
# Path pattern compiler
# ---------------------------------------------------------------------------

def _compile_pattern(pattern: str):
    """Convierte /users/:id/posts a (regex, [param_names])."""
    param_names = []
    parts = []
    for part in pattern.split("/"):
        if part.startswith(":"):
            param_names.append(part[1:])
            parts.append(r"([^/]+)")
        else:
            parts.append(re.escape(part))
    return re.compile("^" + "/".join(parts) + "$"), param_names


# ---------------------------------------------------------------------------
# Router
# ---------------------------------------------------------------------------

class Router:
    """Router HTTP con soporte de path params y métodos HTTP estándar."""

    def __init__(self):
        self._static   = {}   # {(METHOD, path): handler}
        self._patterns = []   # [(METHOD, regex, [param_names], handler)]
        self._not_found_handler = None

    # ── Route registration ──────────────────────────────────────────────────

    def _add(self, method: str, path: str, handler):
        method = method.upper()
        if ":" in path:
            regex, names = _compile_pattern(path)
            self._patterns.append((method, regex, names, handler))
        else:
            self._static[(method, path)] = handler

    def get(self, path, handler):
        self._add("GET", path, handler)
        return self

    def post(self, path, handler):
        self._add("POST", path, handler)
        return self

    def put(self, path, handler):
        self._add("PUT", path, handler)
        return self

    def delete(self, path, handler):
        self._add("DELETE", path, handler)
        return self

    def patch(self, path, handler):
        self._add("PATCH", path, handler)
        return self

    def route(self, method, path, handler):
        self._add(method, path, handler)
        return self

    def not_found(self, handler):
        self._not_found_handler = handler
        return self

    # ── Routing ─────────────────────────────────────────────────────────────

    def _match(self, method: str, path: str):
        """Retorna (handler, url_params) o (None, {})."""
        h = self._static.get((method, path))
        if h:
            return h, {}
        for m, regex, names, handler in self._patterns:
            if m != method:
                continue
            match = regex.match(path)
            if match:
                return handler, dict(zip(names, match.groups()))
        return None, {}

    # ── Server ──────────────────────────────────────────────────────────────

    def listen(self, port):
        """Inicia el servidor HTTP bloqueante en el puerto dado."""
        router = self

        class _Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, fmt, *args):
                pass  # silenciar logs de http.server

            def _handle(self):
                parsed  = urllib.parse.urlparse(self.path)
                path    = parsed.path
                params  = dict(urllib.parse.parse_qsl(parsed.query))
                length  = int(self.headers.get("Content-Length", 0))
                body    = self.rfile.read(length).decode("utf-8", errors="replace") if length else ""
                headers = dict(self.headers)

                handler, url_params = router._match(self.command, path)

                if handler is None:
                    if router._not_found_handler:
                        req = _OrionRequest(self.command, path, body, params, url_params, headers)
                        result = router._not_found_handler(req)
                        self._send(result)
                    else:
                        self.send_response(404)
                        self.send_header("Content-Type", "text/plain; charset=utf-8")
                        self.end_headers()
                        self.wfile.write(b"404 Not Found")
                    return

                req = _OrionRequest(self.command, path, body, params, url_params, headers)
                try:
                    result = handler(req)
                except Exception as e:
                    result = {"status": 500, "body": f"Internal Server Error: {e}"}

                self._send(result)

            def _send(self, result):
                if isinstance(result, dict):
                    if "json" in result and "body" not in result:
                        status = int(result.get("status", 200))
                        body_out = _json.dumps(result["json"], ensure_ascii=False)
                        ct = "application/json; charset=utf-8"
                    else:
                        status = int(result.get("status", 200))
                        body_out = str(result.get("body", ""))
                        ct = result.get("content_type", "text/plain; charset=utf-8")
                else:
                    status = 200
                    body_out = str(result) if result is not None else ""
                    ct = "text/plain; charset=utf-8"

                encoded = body_out.encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", ct)
                self.send_header("Content-Length", str(len(encoded)))
                self.end_headers()
                self.wfile.write(encoded)

            def do_GET(self):    self._handle()
            def do_POST(self):   self._handle()
            def do_PUT(self):    self._handle()
            def do_DELETE(self): self._handle()
            def do_PATCH(self):  self._handle()

        port = int(port)
        server = http.server.ThreadingHTTPServer(("0.0.0.0", port), _Handler)
        print(f"[Orion] Servidor en http://0.0.0.0:{port}  (Ctrl+C para detener)")
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            print("\n[Orion] Servidor detenido.")
        finally:
            server.server_close()


# ---------------------------------------------------------------------------
# Response helpers
# ---------------------------------------------------------------------------

def json_response(data, status=200):
    """Retorna respuesta JSON."""
    return {"status": int(status), "json": data}


def text_response(text, status=200):
    """Retorna respuesta texto plano."""
    return {"status": int(status), "body": str(text), "content_type": "text/plain; charset=utf-8"}


def html_response(html, status=200):
    """Retorna respuesta HTML."""
    return {"status": int(status), "body": str(html), "content_type": "text/html; charset=utf-8"}


def redirect(url, status=302):
    """Retorna una redirección HTTP."""
    return {"status": int(status), "body": "", "content_type": "text/plain", "location": str(url)}


# ---------------------------------------------------------------------------
# Factory
# ---------------------------------------------------------------------------

def create():
    """Crea y retorna un nuevo Router."""
    return Router()


# ---------------------------------------------------------------------------
# Orion module export
# ---------------------------------------------------------------------------

# Aliases cortos para la API pública (load_module los detecta por nombre)
json = json_response
text = text_response
html = html_response


ALIASES = {
    "create":   create,
    "json":     json_response,
    "text":     text_response,
    "html":     html_response,
    "redirect": redirect,
    "Router":   Router,
}


def orion_export():
    return {"server": ALIASES, **ALIASES}
