"""
Orion NET Module
────────────────────────────────────────────
Minimalist, modern and futuristic: HTTP + sockets.
Born to connect. Speaks in pulses, resolves in clarity.

Core principles:
- Clear verbs: reach, transmit, stream, pulse, beacon
- Human-like semantics: not "request", but "reach"
- Designed for cosmic simplicity and expressive power
"""

import requests
import socket
import time

# Opcional: importa httpx si está disponible
try:
    import httpx
except ImportError:
    httpx = None

# OrionResponse: respuesta envolvente
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

# Dummy trace/progress (debes implementar en tu core)
def trace_start(msg): print(f"[TRACE START] {msg}")
def trace_end(msg): print(f"[TRACE END] {msg}")
def progress(tag, msg, percent): print(f"[{tag}] {msg}: {percent}%")

# =========================================================
# HTTP — Orion way
# =========================================================

def reach(url, params=None, headers=None, timeout=5):
    trace_start("NET REACH")
    progress("net", "Connecting", 20)
    resp = requests.get(url, params=params, headers=headers, timeout=timeout)
    trace_end("NET REACH")
    return _pack_response(resp)

def transmit(url, data=None, json_data=None, headers=None, timeout=5):
    trace_start("NET TRANSMIT")
    progress("net", "Transmitting", 20)
    # Solo para pruebas: ignorar verificación SSL
    resp = requests.post(url, data=data, json=json_data, headers=headers, timeout=timeout, verify=False)
    trace_end("NET TRANSMIT")
    return _pack_response(resp)

def stream(url, headers=None, chunk_size=1024, timeout=5): 
    with requests.get(url, headers=headers, stream=True, timeout=timeout) as r:
        for chunk in r.iter_content(chunk_size=chunk_size):
            if chunk:
                yield chunk

def progressive_stream(url, headers=None, chunk_size=1024, timeout=5, progress_fn=None):
    with requests.get(url, headers=headers, stream=True, timeout=timeout) as r:
        total = int(r.headers.get("content-length", 0))
        downloaded = 0
        for chunk in r.iter_content(chunk_size=chunk_size):
            if chunk:
                downloaded += len(chunk)
                if progress_fn:
                    progress_fn(downloaded, total)
                yield chunk

def download(url, path, headers=None, timeout=5):
    with requests.get(url, headers=headers, stream=True, timeout=timeout) as r:
        with open(path, "wb") as f:
            for chunk in r.iter_content(chunk_size=8192):
                f.write(chunk)
    return OrionResponse(status=200, body=None, json=None, headers={"saved": path})

def status(url, timeout=5):
    resp = requests.head(url, timeout=timeout)
    return OrionResponse(status=resp.status_code, body=None, json=None, headers=dict(resp.headers))

def _pack_response(resp):
    return OrionResponse(
        status=resp.status_code,
        body=resp.text,
        json=_transmute_json(resp),
        headers=dict(resp.headers)
    )

def _transmute_json(resp):
    try:
        return resp.json()
    except Exception:
        return None

# =========================================================
# ASYNC — Futuristic Orion
# =========================================================

async def reach_async(url, params=None, headers=None, timeout=5):
    if not httpx:
        raise ImportError("httpx is required for async operations")
    async with httpx.AsyncClient(timeout=timeout) as client:
        resp = await client.get(url, params=params, headers=headers)
        return OrionResponse(
            status=resp.status_code,
            body=resp.text,
            json=resp.json() if resp.headers.get("content-type", "").startswith("application/json") else None,
            headers=dict(resp.headers)
        )

async def transmit_async(url, data=None, json_data=None, headers=None, timeout=5):
    if not httpx:
        raise ImportError("httpx is required for async operations")
    async with httpx.AsyncClient(timeout=timeout) as client:
        resp = await client.post(url, data=data, json=json_data, headers=headers)
        return OrionResponse(
            status=resp.status_code,
            body=resp.text,
            json=resp.json() if resp.headers.get("content-type", "").startswith("application/json") else None,
            headers=dict(resp.headers)
        )

# =========================================================
# SOCKETS — Raw Power
# =========================================================

def resolve(host):
    return socket.gethostbyname(host)

def pulse(host, port=80, timeout=1):
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

def broadcast(host, port=9999, msg="orion-broadcast"):
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        s.sendto(msg.encode("utf-8"), (host, port))
        s.close()
        return {"sent": msg, "to": f"{host}:{port}"}
    except Exception as e:
        return {"error": str(e)}

