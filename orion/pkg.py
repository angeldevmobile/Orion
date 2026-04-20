"""
orion/pkg.py — Gestor de paquetes de Orion

Comandos:
  orion add <pkg>           Instala un paquete
  orion remove <pkg>        Desinstala un paquete
  orion list                Lista los paquetes instalados
  orion search <query>      Busca en el registry
  orion update [pkg]        Actualiza uno o todos los paquetes

Paquetes se instalan en:  <proyecto>/packages/<nombre>.orx
Registry local:           packages/registry.json
Instalados:               packages/installed.json
"""

from __future__ import annotations

import json
import os
import urllib.request
import urllib.error
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Rutas
# ---------------------------------------------------------------------------

def _project_root() -> Path:
    """Raíz del proyecto Orion (donde está packages/)."""
    return Path(os.path.dirname(os.path.dirname(__file__)))


def _packages_dir() -> Path:
    return _project_root() / "packages"


def _registry_path() -> Path:
    return _packages_dir() / "registry.json"


def _installed_path() -> Path:
    return _packages_dir() / "installed.json"


# ---------------------------------------------------------------------------
# Registry
# ---------------------------------------------------------------------------

_REGISTRY_REMOTE = (
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages/registry.json"
)
_PKG_REMOTE_BASE = (
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages"
)

def _load_registry(refresh: bool = False) -> dict:
    """Carga el registry local. Si refresh=True intenta actualizar desde GitHub."""
    local = _registry_path()

    if refresh or not local.exists():
        try:
            with urllib.request.urlopen(_REGISTRY_REMOTE, timeout=8) as resp:
                data = json.loads(resp.read().decode("utf-8"))
            local.parent.mkdir(parents=True, exist_ok=True)
            local.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
            return data
        except Exception:
            pass  # fallback al local

    if local.exists():
        return json.loads(local.read_text(encoding="utf-8"))

    return {"packages": {}}


def _load_installed() -> dict:
    """Carga el archivo de paquetes instalados."""
    p = _installed_path()
    if p.exists():
        return json.loads(p.read_text(encoding="utf-8"))
    return {}


def _save_installed(data: dict) -> None:
    p = _installed_path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")


# ---------------------------------------------------------------------------
# Descarga de archivos .orx
# ---------------------------------------------------------------------------

def _download_orx(pkg_name: str, file_name: str) -> bytes:
    """Descarga un archivo .orx desde el repositorio remoto."""
    url = f"{_PKG_REMOTE_BASE}/{file_name}"
    try:
        with urllib.request.urlopen(url, timeout=15) as resp:
            return resp.read()
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"No se pudo descargar '{pkg_name}' ({e.code}): {url}")
    except urllib.error.URLError as e:
        raise RuntimeError(f"Sin conexión o URL inválida para '{pkg_name}': {e.reason}")


# ---------------------------------------------------------------------------
# Comandos públicos
# ---------------------------------------------------------------------------

def add(pkg_name: str, *, force: bool = False) -> str:
    """
    Instala un paquete. Si ya está instalado y force=False, informa sin reinstalar.
    Retorna mensaje de resultado.
    """
    installed = _load_installed()

    if pkg_name in installed and not force:
        v = installed[pkg_name].get("version", "?")
        return f"[ya instalado] {pkg_name} v{v}  — usa --force para reinstalar"

    registry = _load_registry()
    packages = registry.get("packages", {})

    if pkg_name not in packages:
        # Intento con registry remoto actualizado
        registry = _load_registry(refresh=True)
        packages = registry.get("packages", {})
        if pkg_name not in packages:
            available = ", ".join(sorted(packages.keys()))
            return (
                f"[error] Paquete '{pkg_name}' no encontrado en el registry.\n"
                f"Disponibles: {available}"
            )

    meta = packages[pkg_name]
    file_name = meta["file"]
    pkg_type = meta.get("type", "community")
    dest = _packages_dir() / file_name

    # Si el archivo ya existe en disco (builtin o bundled) — sólo registrar
    if dest.exists() and not force:
        installed[pkg_name] = {
            "version":     meta["version"],
            "description": meta["description"],
            "file":        file_name,
            "source":      pkg_type,
        }
        _save_installed(installed)
        return f"[ok] {pkg_name} v{meta['version']} instalado  → packages/{file_name}"

    # Intentar descarga remota
    try:
        content = _download_orx(pkg_name, file_name)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(content)
    except RuntimeError:
        # Sin conexión — si existe local, usar ese
        if dest.exists():
            installed[pkg_name] = {
                "version":     meta["version"],
                "description": meta["description"],
                "file":        file_name,
                "source":      "local",
            }
            _save_installed(installed)
            return f"[ok] {pkg_name} v{meta['version']} instalado (sin conexión, versión local)"
        return f"[error] '{pkg_name}' no está disponible localmente y no hay conexión"

    installed[pkg_name] = {
        "version":     meta["version"],
        "description": meta["description"],
        "file":        file_name,
        "source":      "remote",
    }
    _save_installed(installed)
    return f"[ok] {pkg_name} v{meta['version']} instalado  → packages/{file_name}"


def remove(pkg_name: str, *, keep_file: bool = False) -> str:
    """Desinstala un paquete. Por defecto elimina también el .orx (excepto builtins)."""
    installed = _load_installed()

    if pkg_name not in installed:
        return f"[error] '{pkg_name}' no está instalado"

    meta = installed.pop(pkg_name)
    _save_installed(installed)

    # No borrar archivos builtin (vienen con Orion)
    if meta.get("source") == "builtin" or keep_file:
        return f"[ok] {pkg_name} desregistrado (archivo conservado)"

    pkg_file = _packages_dir() / meta["file"]
    if pkg_file.exists():
        pkg_file.unlink()
        return f"[ok] {pkg_name} desinstalado y archivo eliminado"

    return f"[ok] {pkg_name} desregistrado"


def list_installed() -> list[dict]:
    """Retorna la lista de paquetes instalados como lista de dicts."""
    installed = _load_installed()
    return [
        {"name": k, **v}
        for k, v in sorted(installed.items())
    ]


def search(query: str) -> list[dict]:
    """
    Busca en el registry por nombre, descripción o tags.
    Retorna lista de dicts con los resultados ordenados por relevancia.
    """
    query_lower = query.lower()
    registry = _load_registry()
    packages = registry.get("packages", {})

    results = []
    for name, meta in packages.items():
        score = 0
        if query_lower in name.lower():
            score += 10
        if query_lower in meta.get("description", "").lower():
            score += 5
        for tag in meta.get("tags", []):
            if query_lower in tag.lower():
                score += 3
        if score > 0:
            results.append({"name": name, "score": score, **meta})

    results.sort(key=lambda x: x["score"], reverse=True)
    return results


def update(pkg_name: Optional[str] = None) -> list[str]:
    """
    Actualiza un paquete o todos los instalados.
    Retorna lista de mensajes de resultado.
    """
    installed = _load_installed()
    if not installed:
        return ["[info] No hay paquetes instalados"]

    targets = [pkg_name] if pkg_name else list(installed.keys())
    messages = []

    for name in targets:
        if name not in installed:
            messages.append(f"[error] '{name}' no está instalado")
            continue
        msg = add(name, force=True)
        messages.append(msg)

    return messages
