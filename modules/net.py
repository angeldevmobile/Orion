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

# =========================================================
# HTTP — Orion way
# =========================================================

def reach(url, params=None, headers=None):
    """Reaches out to a URL with GET."""
    resp = requests.get(url, params=params, headers=headers)
    return _pack_response(resp)

def transmit(url, data=None, json_data=None, headers=None):
    """Transmits data to a URL with POST."""
    resp = requests.post(url, data=data, json=json_data, headers=headers)
    return _pack_response(resp)

def stream(url, headers=None, chunk_size=1024):
    """Streams data from a URL (cosmic flow)."""
    with requests.get(url, headers=headers, stream=True) as r:
        for chunk in r.iter_content(chunk_size=chunk_size):
            if chunk:
                yield chunk

def download(url, path, headers=None):
    """Downloads a resource from the web."""
    with requests.get(url, headers=headers, stream=True) as r:
        with open(path, "wb") as f:
            for chunk in r.iter_content(chunk_size=8192):
                f.write(chunk)
    return {"status": "saved", "path": path}

def status(url):
    """Quick status probe of a URL."""
    resp = requests.head(url)
    return {"status": resp.status_code, "headers": dict(resp.headers)}

def _pack_response(resp):
    return {
        "status": resp.status_code,
        "body": resp.text,
        "json": _transmute_json(resp),
        "headers": dict(resp.headers)
    }

def _transmute_json(resp):
    try:
        return resp.json()
    except Exception:
        return None


# =========================================================
# SOCKETS — Raw Power
# =========================================================

def resolve(host):
    """Resolves a hostname into its raw IP essence."""
    return socket.gethostbyname(host)

def pulse(host, port=80, timeout=1):
    """Sends a fast TCP pulse (like a futuristic ping)."""
    try:
        start = time.time()
        with socket.create_connection((host, port), timeout=timeout):
            latency = (time.time() - start) * 1000
            return {"alive": True, "latency_ms": round(latency, 2)}
    except Exception:
        return {"alive": False}

def beacon(host, port=80, msg="orion-signal", timeout=1):
    """
    Sends a message to the cosmos and awaits reply.
    Beacon is Orion’s futuristic ping: not just reach, but converse.
    """
    try:
        with socket.create_connection((host, port), timeout=timeout) as s:
            s.sendall(msg.encode("utf-8"))
            data = s.recv(1024)
            return {"reply": data.decode("utf-8", errors="ignore")}
    except Exception as e:
        return {"error": str(e)}

def broadcast(host, port=9999, msg="orion-broadcast"):
    """Sends a UDP broadcast signal."""
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        s.sendto(msg.encode("utf-8"), (host, port))
        s.close()
        return {"sent": msg, "to": f"{host}:{port}"}
    except Exception as e:
        return {"error": str(e)}
