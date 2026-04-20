"""
Orion NET Module
────────────────────────────────────────────
HTTP real usando urllib (sin dependencias externas).
requests / httpx son opcionales y se usan si están disponibles.

Verbos: reach, transmit, stream, pulse, beacon
"""

import socket
import time
import json as _json
import urllib.request
import urllib.error
import urllib.parse

# Intentar importar requests/httpx como opcional
try:
    import requests as _requests
except ImportError:
    _requests = None

try:
    import httpx as _httpx
except ImportError:
    _httpx = None


class OrionResponse:
    def __init__(self, status, body=None, json=None, headers=None):
        self.status = status
        self.body = body
        self.json = json
        self.headers = headers or {}

    @property
    def ok(self):
        return 200 <= self.status < 300

    def __getitem__(self, key):
        return getattr(self, key, None)

    def __repr__(self):
        return f"<OrionResponse {self.status}>"


# =========================================================
# HTTP — implementación urllib (sin dependencias)
# =========================================================

def _urllib_get(url, params=None, headers=None, timeout=5):
    if params:
        url = url + "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(url, headers=headers or {})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            status = resp.status
            hdrs = dict(resp.headers)
            json_data = None
            try:
                json_data = _json.loads(body)
            except Exception:
                pass
            return OrionResponse(status=status, body=body, json=json_data, headers=hdrs)
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        return OrionResponse(status=e.code, body=body, json=None, headers=dict(e.headers))
    except urllib.error.URLError as e:
        raise RuntimeError(f"net.reach error: {e.reason}")


def _urllib_post(url, data=None, json_data=None, headers=None, timeout=5):
    hdrs = headers or {}
    if json_data is not None:
        payload = _json.dumps(json_data).encode("utf-8")
        hdrs.setdefault("Content-Type", "application/json")
    elif data is not None:
        payload = urllib.parse.urlencode(data).encode("utf-8") if isinstance(data, dict) else data.encode("utf-8")
    else:
        payload = b""
    req = urllib.request.Request(url, data=payload, headers=hdrs, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            status = resp.status
            resp_hdrs = dict(resp.headers)
            json_body = None
            try:
                json_body = _json.loads(body)
            except Exception:
                pass
            return OrionResponse(status=status, body=body, json=json_body, headers=resp_hdrs)
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        return OrionResponse(status=e.code, body=body, json=None, headers=dict(e.headers))
    except urllib.error.URLError as e:
        raise RuntimeError(f"net.transmit error: {e.reason}")


# =========================================================
# API pública
# =========================================================

def reach(url, params=None, headers=None, timeout=5):
    """GET a url. Devuelve OrionResponse."""
    if _requests:
        resp = _requests.get(url, params=params, headers=headers, timeout=timeout)
        return _pack_requests(resp)
    return _urllib_get(url, params=params, headers=headers, timeout=timeout)


def transmit(url, data=None, json_data=None, headers=None, timeout=5):
    """POST a url. Devuelve OrionResponse."""
    if _requests:
        resp = _requests.post(url, data=data, json=json_data, headers=headers, timeout=timeout)
        return _pack_requests(resp)
    return _urllib_post(url, data=data, json_data=json_data, headers=headers, timeout=timeout)


def status(url, timeout=5):
    """HEAD request — solo código de estado."""
    if _requests:
        resp = _requests.head(url, timeout=timeout)
        return OrionResponse(status=resp.status_code, headers=dict(resp.headers))
    req = urllib.request.Request(url, method="HEAD")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return OrionResponse(status=resp.status, headers=dict(resp.headers))
    except urllib.error.HTTPError as e:
        return OrionResponse(status=e.code)


def download(url, path, headers=None, timeout=5):
    """Descarga un archivo y lo guarda en path."""
    if _requests:
        with _requests.get(url, headers=headers, stream=True, timeout=timeout) as r:
            with open(path, "wb") as f:
                for chunk in r.iter_content(chunk_size=8192):
                    f.write(chunk)
    else:
        req = urllib.request.Request(url, headers=headers or {})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            with open(path, "wb") as f:
                while True:
                    chunk = resp.read(8192)
                    if not chunk:
                        break
                    f.write(chunk)
    return OrionResponse(status=200, headers={"saved": path})


# =========================================================
# Sockets — raw power
# =========================================================

def resolve(host):
    return socket.gethostbyname(host)


def pulse(host, port=80, timeout=1):
    """Comprueba si host:port está vivo y mide latencia."""
    try:
        start = time.time()
        with socket.create_connection((host, port), timeout=timeout):
            latency = (time.time() - start) * 1000
            return {"alive": True, "latency_ms": round(latency, 2)}
    except Exception:
        return {"alive": False}


def beacon(host, port=80, msg="orion-signal", timeout=1):
    try:
        with socket.create_connection((host, port), timeout=timeout) as s:
            s.sendall(msg.encode("utf-8"))
            data = s.recv(1024)
            return {"reply": data.decode("utf-8", errors="ignore")}
    except Exception as e:
        return {"error": str(e)}


# =========================================================
# Async (requiere httpx)
# =========================================================

async def reach_async(url, params=None, headers=None, timeout=5):
    if not _httpx:
        raise ImportError("httpx requerido para operaciones async: pip install httpx")
    async with _httpx.AsyncClient(timeout=timeout) as client:
        resp = await client.get(url, params=params, headers=headers)
        return OrionResponse(
            status=resp.status_code,
            body=resp.text,
            json=resp.json() if "application/json" in resp.headers.get("content-type", "") else None,
            headers=dict(resp.headers),
        )


async def transmit_async(url, data=None, json_data=None, headers=None, timeout=5):
    if not _httpx:
        raise ImportError("httpx requerido para operaciones async: pip install httpx")
    async with _httpx.AsyncClient(timeout=timeout) as client:
        resp = await client.post(url, data=data, json=json_data, headers=headers)
        return OrionResponse(
            status=resp.status_code,
            body=resp.text,
            json=resp.json() if "application/json" in resp.headers.get("content-type", "") else None,
            headers=dict(resp.headers),
        )


# =========================================================
# Helper interno
# =========================================================

def _pack_requests(resp):
    json_data = None
    try:
        json_data = resp.json()
    except Exception:
        pass
    return OrionResponse(
        status=resp.status_code,
        body=resp.text,
        json=json_data,
        headers=dict(resp.headers),
    )
