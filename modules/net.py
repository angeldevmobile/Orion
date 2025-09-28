"""
Módulo de red para Orion.
Minimalista, moderno y futurista: HTTP requests y sockets.
"""
import requests
import socket

# --- HTTP ---
def get(url, params=None, headers=None):
    resp = requests.get(url, params=params, headers=headers)
    return {"status": resp.status_code, "body": resp.text, "json": try_json(resp)}

def post(url, data=None, json_data=None, headers=None):
    resp = requests.post(url, data=data, json=json_data, headers=headers)
    return {"status": resp.status_code, "body": resp.text, "json": try_json(resp)}

def try_json(resp):
    try:
        return resp.json()
    except Exception:
        return None

# --- Raw sockets ---
def resolve(host):
    """Resuelve un hostname a IP."""
    return socket.gethostbyname(host)

def ping(host, port=80, timeout=1):
    """Intenta conexión TCP rápida (tipo ping)."""
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except Exception:
        return False

# --- Futurista ---
def beacon(host, port=80, msg="orion-signal", timeout=1):
    """
    Envía un mensaje simple y recibe respuesta.
    Futurista: como enviar un 'ping' cósmico.
    """
    try:
        with socket.create_connection((host, port), timeout=timeout) as s:
            s.sendall(msg.encode("utf-8"))
            data = s.recv(1024)
            return data.decode("utf-8", errors="ignore")
    except Exception as e:
        return f"Error: {e}"
