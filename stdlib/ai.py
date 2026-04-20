"""
Orion AI Module — Fase 4
Real AI via Claude (Anthropic) or OpenAI.
Auto-detects provider from .env or environment variables.
No external dependencies — solo stdlib Python.
"""

import json
import os
import urllib.request
import urllib.error
from pathlib import Path


# ---------------------------------------------------------------------------
# .env loader — seguro, sin eval/exec
# ---------------------------------------------------------------------------

def _load_env():
    """Carga variables de un archivo .env buscando desde el directorio actual hacia arriba."""
    search_paths = [Path(".env")]
    for level in range(1, 4):
        search_paths.append(Path(*[".."] * level) / ".env")

    for env_path in search_paths:
        try:
            with open(env_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    if "=" in line:
                        key, _, value = line.partition("=")
                        key   = key.strip()
                        value = value.strip().strip('"').strip("'")
                        if key and key not in os.environ:
                            os.environ[key] = value
            return
        except FileNotFoundError:
            continue

_load_env()

# Modelo activo en runtime (puede cambiarse desde Orion con ai.set_model())
_active_model: str | None = None

# ---------------------------------------------------------------------------
# Memoria de sesión  — usada por learn / sense
# ---------------------------------------------------------------------------
_session_memory: list[str] = []


def embed(text: str) -> str:
    """Guarda texto en la memoria de sesión (usado por el statement 'learn')."""
    _session_memory.append(str(text))
    return f"[aprendido: {len(_session_memory)} entradas en memoria]"


def recall(query: str) -> str:
    """Busca en la memoria de sesión y responde usando AI (usado por 'sense')."""
    if not _session_memory:
        return "[sense: memoria vacía — usa 'learn' primero]"
    context = "\n---\n".join(_session_memory)
    return _call(
        [{"role": "user", "content": str(query)}],
        system=(
            "Responde usando ÚNICAMENTE la siguiente información almacenada:\n\n"
            f"{context}\n\n"
            "Si la respuesta no está en la información, dilo claramente."
        ),
        max_tokens=512,
    )


def memory_size() -> int:
    """Retorna cuántas entradas hay en memoria de sesión."""
    return len(_session_memory)


def memory_clear() -> str:
    """Limpia la memoria de sesión."""
    _session_memory.clear()
    return "[memoria borrada]"


# ---------------------------------------------------------------------------
# Detección de proveedor
# ---------------------------------------------------------------------------

def _get_provider() -> str | None:
    pref       = os.environ.get("AI_MODEL", "auto").lower()
    has_claude = bool(os.environ.get("ANTHROPIC_API_KEY"))
    has_openai = bool(os.environ.get("OPENAI_API_KEY"))

    if pref == "claude"  and has_claude: return "anthropic"
    if pref == "openai"  and has_openai: return "openai"
    if has_claude: return "anthropic"
    if has_openai: return "openai"
    return None


def _require_provider():
    p = _get_provider()
    if not p:
        raise RuntimeError(
            "No hay API key configurada.\n"
            "Agrega en tu .env:\n"
            "  ANTHROPIC_API_KEY=sk-ant-...\n"
            "o\n"
            "  OPENAI_API_KEY=sk-..."
        )
    return p


# ---------------------------------------------------------------------------
# HTTP helpers — sin requests, solo urllib
# ---------------------------------------------------------------------------

def _http_post(url: str, headers: dict, body: dict, timeout: int = 30) -> dict:
    data = json.dumps(body).encode("utf-8")
    req  = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", errors="replace")
        try:
            detail = json.loads(raw).get("error", {}).get("message", raw[:300])
        except Exception:
            detail = raw[:300]
        raise RuntimeError(f"API error ({e.code}): {detail}")


def _resolve_model(provider: str, override: str = None) -> str:
    """Retorna el modelo a usar: override > set_model() > .env
    Si ninguno está configurado, lanza error claro."""
    if override:
        return override
    if _active_model:
        return _active_model
    env_key = "ANTHROPIC_MODEL" if provider == "anthropic" else "OPENAI_MODEL"
    m = os.environ.get(env_key)
    if not m:
        raise RuntimeError(
            f"Modelo no configurado. Usa ai.set_model('nombre-del-modelo') "
            f"o agrega {env_key}=... en tu .env"
        )
    return m


def _call_anthropic(messages: list, system: str = None, max_tokens: int = 1024, model: str = None) -> str:
    key = os.environ.get("ANTHROPIC_API_KEY")
    if not key:
        raise RuntimeError("ANTHROPIC_API_KEY no configurada en .env")

    body = {
        "model":      _resolve_model("anthropic", model),
        "max_tokens": max_tokens,
        "messages":   messages,
    }
    if system:
        body["system"] = system

    result = _http_post(
        "https://api.anthropic.com/v1/messages",
        headers={
            "Content-Type":    "application/json",
            "x-api-key":       key,
            "anthropic-version": "2023-06-01",
        },
        body=body,
    )
    return result["content"][0]["text"]


def _call_openai(messages: list, system: str = None, max_tokens: int = 1024, model: str = None) -> str:
    key = os.environ.get("OPENAI_API_KEY")
    if not key:
        raise RuntimeError("OPENAI_API_KEY no configurada en .env")

    msgs = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.extend(messages)

    result = _http_post(
        "https://api.openai.com/v1/chat/completions",
        headers={
            "Content-Type":  "application/json",
            "Authorization": f"Bearer {key}",
        },
        body={
            "model":      _resolve_model("openai", model),
            "max_tokens": max_tokens,
            "messages":   msgs,
        },
    )
    return result["choices"][0]["message"]["content"]


def _call(messages: list, system: str = None, max_tokens: int = 1024, model: str = None) -> str:
    p = _require_provider()
    if p == "anthropic":
        return _call_anthropic(messages, system, max_tokens, model)
    return _call_openai(messages, system, max_tokens, model)


def _clean_json(raw: str) -> str:
    """Elimina bloques de código markdown si el modelo los incluyó."""
    raw = raw.strip()
    if raw.startswith("```"):
        lines = raw.split("\n")
        lines = lines[1:]  # quitar ```json o ```
        if lines and lines[-1].strip() == "```":
            lines = lines[:-1]
        raw = "\n".join(lines).strip()
    return raw


# ---------------------------------------------------------------------------
# API pública
# ---------------------------------------------------------------------------

def set_model(name: str) -> str:
    """Cambia el modelo activo en runtime. Ej: ai.set_model('claude-sonnet-4-6')"""
    global _active_model
    _active_model = str(name)
    return _active_model


def ask(prompt: str, model: str = None, max_tokens: int = 1024) -> str:
    """Pregunta algo al modelo y retorna la respuesta."""
    return _call([{"role": "user", "content": str(prompt)}], max_tokens=max_tokens, model=model)


def summarize(text: str, lang: str = "español", length: str = "corto") -> str:
    """Resume un texto. length: 'corto' | 'medio' | 'largo'"""
    sizes = {"corto": 256, "medio": 512, "largo": 1024}
    max_t = sizes.get(length, 256)
    return _call(
        [{"role": "user", "content": f"Resume este texto de forma {length}:\n\n{text}"}],
        system=f"Eres un asistente que resume textos en {lang}. Sé conciso y claro.",
        max_tokens=max_t,
    )


def classify(text: str, categories: list) -> str:
    """Clasifica texto en una de las categorías dadas. Retorna el nombre exacto."""
    cats = ", ".join(str(c) for c in categories)
    result = _call(
        [{"role": "user", "content": str(text)}],
        system=f"Clasifica el texto en UNA de estas categorías: {cats}. Responde SOLO con el nombre de la categoría, sin explicación ni puntuación extra.",
        max_tokens=32,
    )
    return result.strip()


def extract(text: str, fields: list) -> dict:
    """Extrae campos estructurados de un texto. Retorna un dict."""
    result = _call(
        [{"role": "user", "content": str(text)}],
        system=f"Extrae los campos {json.dumps(fields)} del texto. Responde SOLO con JSON válido. Si un campo no existe usa null.",
        max_tokens=512,
    )
    try:
        return json.loads(_clean_json(result))
    except json.JSONDecodeError:
        return {"raw": result}


def code(description: str, lang: str = "orion") -> str:
    """Genera código según la descripción. Retorna solo el código."""
    return _call(
        [{"role": "user", "content": str(description)}],
        system=f"Genera código en {lang}. Responde SOLO con el código, sin explicaciones ni bloques markdown.",
        max_tokens=1024,
    )


def fix(code_text: str, error: str = "", lang: str = "auto") -> str:
    """Corrige errores en código."""
    content = f"Código:\n{code_text}"
    if error:
        content += f"\n\nError:\n{error}"
    return _call(
        [{"role": "user", "content": content}],
        system="Corrige el código. Responde SOLO con el código corregido, sin explicaciones.",
        max_tokens=1024,
    )


def translate(text: str, to: str = "english") -> str:
    """Traduce texto a otro idioma."""
    return _call(
        [{"role": "user", "content": str(text)}],
        system=f"Traduce al {to}. Responde SOLO con la traducción.",
        max_tokens=1024,
    )


def sentiment(text: str) -> str:
    """Analiza el sentimiento. Retorna: 'positivo', 'negativo' o 'neutro'."""
    result = _call(
        [{"role": "user", "content": str(text)}],
        system="Analiza el sentimiento. Responde SOLO con una palabra: positivo, negativo, o neutro.",
        max_tokens=8,
    )
    return result.strip().lower()


def complete(text: str, max_tokens: int = 256) -> str:
    """Completa un texto o fragmento de código de forma natural."""
    return _call(
        [{"role": "user", "content": str(text)}],
        system="Continúa el texto o código de forma natural y coherente. Responde SOLO con la continuación.",
        max_tokens=max_tokens,
    )


def improve(text: str) -> str:
    """Mejora la redacción o calidad de un texto."""
    return _call(
        [{"role": "user", "content": str(text)}],
        system="Mejora la redacción, claridad y calidad del texto. Responde SOLO con el texto mejorado.",
        max_tokens=1024,
    )


def explain(code_text: str, lang: str = "español") -> str:
    """Explica qué hace un fragmento de código."""
    return _call(
        [{"role": "user", "content": f"Explica este código:\n\n{code_text}"}],
        system=f"Eres un experto programador. Explica el código en {lang} de forma clara y concisa.",
        max_tokens=512,
    )


def qa(context: str, question: str) -> str:
    """Responde una pregunta basándose en un contexto dado."""
    return _call(
        [{"role": "user", "content": f"Contexto:\n{context}\n\nPregunta: {question}"}],
        system="Responde SOLO con base en el contexto dado. Si la respuesta no está en el contexto, dilo.",
        max_tokens=512,
    )


def search_in(text: str, query: str) -> str:
    """Busca información específica dentro de un texto largo."""
    return _call(
        [{"role": "user", "content": f"Texto:\n{text}\n\nBusca: {query}"}],
        system="Encuentra y extrae la información solicitada del texto. Sé directo y preciso.",
        max_tokens=256,
    )


# ---------------------------------------------------------------------------
# Chat session — mantiene historial de conversación
# ---------------------------------------------------------------------------

class Chat:
    """Sesión de chat con memoria de contexto."""

    def __init__(self, system_prompt: str = ""):
        self._system  = system_prompt
        self._history = []

    def say(self, message: str) -> "Chat":
        """Define el rol del asistente (system prompt)."""
        self._system = str(message)
        return self

    def ask(self, prompt: str, max_tokens: int = 1024) -> str:
        """Envía un mensaje y retorna la respuesta. Recuerda el historial."""
        self._history.append({"role": "user", "content": str(prompt)})
        response = _call(
            self._history,
            system=self._system or None,
            max_tokens=max_tokens,
        )
        self._history.append({"role": "assistant", "content": response})
        return response

    def reset(self) -> "Chat":
        """Limpia el historial manteniendo el system prompt."""
        self._history = []
        return self

    def history(self) -> list:
        """Retorna el historial completo de la conversación."""
        return list(self._history)

    def messages(self) -> int:
        """Retorna el número de mensajes en el historial."""
        return len(self._history)


def chat(system_prompt: str = "") -> Chat:
    """Crea una nueva sesión de chat con contexto persistente."""
    return Chat(system_prompt)


# ---------------------------------------------------------------------------
# Info del proveedor activo
# ---------------------------------------------------------------------------

def provider() -> str:
    """Retorna el proveedor activo: 'anthropic', 'openai', o 'none'."""
    return _get_provider() or "none"


def model() -> str:
    """Retorna el modelo activo (runtime > .env > default)."""
    p = _get_provider()
    if p:
        return _resolve_model(p)
    return "none"


def status() -> str:
    """Muestra el estado de la configuración AI."""
    p = _get_provider()
    if p:
        return f"AI activo — proveedor: {p}, modelo: {model()}"
    return "AI no configurado. Agrega ANTHROPIC_API_KEY o OPENAI_API_KEY en tu .env"


# ---------------------------------------------------------------------------
# Exportar al intérprete Orion
# ---------------------------------------------------------------------------

def orion_export() -> dict:
    return {
        "ask":          ask,
        "summarize":    summarize,
        "classify":     classify,
        "extract":      extract,
        "code":         code,
        "fix":          fix,
        "translate":    translate,
        "sentiment":    sentiment,
        "complete":     complete,
        "improve":      improve,
        "explain":      explain,
        "qa":           qa,
        "search_in":    search_in,
        "chat":         chat,
        "set_model":    set_model,
        "provider":     provider,
        "model":        model,
        "status":       status,
        # memoria de sesión
        "embed":        embed,
        "recall":       recall,
        "memory_size":  memory_size,
        "memory_clear": memory_clear,
    }


__all__ = list(orion_export().keys())
