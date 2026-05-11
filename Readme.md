# Orion Language

Orion es un lenguaje de programación moderno orientado a backend y automatización.
Sintaxis limpia, tipado opcional, OOP nativa, 20 módulos integrados y pipeline completo en Rust.

> Construido por **Angel Zapata** · 2025–2026

---

## Filosofía

- **Sin boilerplate** — el código se lee como pseudocódigo. Una tarea = máximo 5 líneas.
- **Real** — pensado para construir cosas reales: APIs, automatizaciones, pipelines de datos.
- **Moderno** — OOP, type hints, interpolación, async/await, IA nativa, regex, AI como keyword.
- **Rápido** — pipeline completo en Rust: lexer → parser → type checker → codegen → VM.
- **Seguro** — queries parametrizadas, validación en frontera, crypto nativo.

---

## Instalación

```bash
# Compilar desde fuente
cargo build --release --manifest-path orion-vm/Cargo.toml

# Ejecutar
./orion-vm/target/release/orion archivo.orx
```

**Extensión VSCode** — incluye el binario bundleado, zero-config:
1. Instalar `orion-lang` desde el marketplace
2. Abrir cualquier `.orx` — funciona de inmediato

---

## Sintaxis

### Variables y tipos

```orion
-- Variables
nombre = "Orion"
edad   = 25
activo = yes

-- Constantes
const PI = 3.14159

-- Type hints opcionales
ciudad:  string = "Monterrey"
version: int    = 1

-- Mostrar valores
show nombre
show "Hola " + nombre
show "Versión ${version} de ${nombre}"   -- interpolación

-- Escape sequences
ruta    = "C:\\usuarios\\documentos"
linea   = "nombre\tapellido\nedad"
patron  = "\\d{4}-\\d{2}-\\d{2}"        -- regex: \d{4}-\d{2}-\d{2}
```

### Tipos de datos

| Tipo | Ejemplo | Descripción |
|---|---|---|
| `int` | `42`, `0xFF`, `0b1010` | Entero 64-bit, hex y binario |
| `float` | `3.14`, `1.5e-3` | Decimal, notación científica |
| `string` | `"hola"`, `r"raw"`, `"""multi"""` | Texto con interpolación `${var}` |
| `bool` | `yes` / `no` | Booleano |
| `list` | `[1, 2, 3]` | Array dinámico |
| `dict` | `{"k": "v"}` | Hash map |
| `null` | `null` | Nulo explícito |
| shape | `Persona("Ana", 30)` | Instancia de shape (objeto) |

### Control de flujo

```orion
-- if / elsif / else
if edad >= 18 {
    show "Mayor de edad"
} elsif edad >= 13 {
    show "Adolescente"
} else {
    show "Menor"
}

-- while
i = 0
while i < 5 {
    show i
    i += 1
}

-- for en rango
for x in 1..10 { show x }

-- for en colección
for n in ["Ana", "Luis", "Eva"] { show n }

-- match
resultado = match valor {
    1    => "uno"
    2    => "dos"
    _    => "otro"
}

-- break / continue
for i in 1..100 {
    if i == 10 { break }
    if i % 2 == 0 { continue }
    show i
}
```

### Funciones

```orion
-- Función simple
fn saludar(nombre) {
    return "Hola " + nombre
}

-- Con type hints
fn suma(a: int, b: int) -> int {
    return a + b
}

-- Lambda
doble = fn(x) { x * 2 }
show doble(21)   -- 42

-- Async
async fn fetch(url) {
    resp = net.get(url)
    return resp.body
}
datos = await fetch("https://api.ejemplo.com")
```

### OOP — Shapes

```orion
shape Persona {
    nombre: string = ""
    edad:   int    = 0

    on_create(n: string, e: int) {
        nombre = n
        edad   = e
    }

    act saludar() {
        show "Hola, soy " + nombre
    }

    act cumpleanos() {
        edad += 1
    }
}

p = Persona("Gabriel", 25)
p.saludar()
p.cumpleanos()
show p.edad    -- 26

if p is Persona { show "Es una Persona" }

-- Composición con using
shape Animal {
    nombre: string = ""
    act hablar() { show nombre + " habla" }
}

shape Perro {
    using Animal
    raza: string = ""
    on_create(n, r) { nombre = n   raza = r }
    act buscar() { show nombre + " busca la pelota!" }
}

d = Perro("Rex", "Labrador")
d.hablar()
d.buscar()
```

### Manejo de errores

```orion
attempt {
    resultado = dividir(10, 0)
    show resultado
} handle err {
    show "Error: " + err
}
```

### Servidor HTTP nativo

```orion
serve en 8080 {
    route "GET /ping" {
        responder("pong")
    }

    route "POST /usuarios" {
        db.insertar("usuarios", body)
        responder({ok: yes, mensaje: "Creado"})
    }

    route "GET /usuarios/:id" {
        usuario = db.buscar("usuarios", id)
        responder(usuario)
    }
}
```

### IA nativa — `think`, `learn`, `sense`

```orion
-- Sin módulos, sin imports: IA como statement nativo
think "Resume este texto en 3 puntos: " + contenido

-- Con módulo ai para operaciones avanzadas
use "ai" as ai

categoria  = ai.classify(email.texto, ["spam", "trabajo", "personal"])
resumen    = ai.summarize(documento, length: "corto")
traduccion = ai.translate(texto, to: "english")
sentimiento = ai.sentiment(reseña)   -- "positivo" / "negativo" / "neutro"
```

### Pipe operator

```orion
resultado = datos
    |> filtrar("activo", yes)
    |> ordenar("fecha", "desc")
    |> top(10)
```

### Concurrencia

```orion
-- Spawn (fire and forget)
spawn proceso_largo()

-- Async/await
async fn procesar(item) { ... }
resultado = await procesar(datos)
```

---

## Módulos stdlib (20 módulos)

### Datos y archivos

```orion
use "fs"
use "csv"
use "json"
use "excel"
use "table"
use "regex" as re
```

#### `fs` — Sistema de archivos
```orion
contenido = fs.read("config.toml")
fs.write("output.json", datos)
archivos  = fs.ls("data/")
fs.copy("a.txt", "backup/a.txt")
fs.mkdir("reportes/2026")
info = fs.info("archivo.txt")   -- {size, modified, is_file}
```

#### `csv` — Datos tabulares
```orion
data    = csv.read("ventas.csv")
norte   = csv.filter(data, "region", "Norte")
stats   = csv.stats(data, "venta")   -- {sum, avg, min, max}
ordenado = csv.sort(data, "venta", "desc")
csv.write("reporte.csv", data)
```

#### `json` — Serialización JSON
```orion
obj  = json.parse(texto)
txt  = json.forge_pretty(obj)
data = json.absorb("config.json")
json.emit("salida.json", data)
val  = json.trace(obj, "usuario.perfil.nombre")
```

#### `excel` — Hojas de cálculo
```orion
hojas = excel.sheets("reporte.xlsx")
data  = excel.read("datos.xlsx", "Ventas")
excel.write("salida.xlsx", datos, "Reporte 2026")
```

#### `table` — Análisis de datos
```orion
t = table.load("datos.csv")   -- auto-detecta CSV/Excel/JSON
table.peek(t, 5)              -- imprime las primeras 5 filas bonito
table.schema(t)               -- tipos de cada columna
table.profile(t)              -- estadísticas completas

t2 = table.filter(t, "activo", yes)
t3 = table.keep(t, ["nombre", "venta", "region"])
t4 = table.sort(t, "venta")
t5 = table.join(t, t2, "id")
```

#### `regex` — Expresiones regulares
```orion
use "regex" as re

valido   = re.is_match("usuario@ejemplo.com", "^[\\w.]+@[\\w]+\\.[\\w]+$")
numeros  = re.find_all(texto, "\\d+")
limpio   = re.replace(sucio, "\\s+", " ")
partes   = re.groups("2026-05-08", "(\\d{4})-(\\d{2})-(\\d{2})")
palabras = re.split(linea, "[,;]+")
```

### Red y servidor

```orion
use "net"
use "env"
```

#### `net` — HTTP client
```orion
resp  = net.get("https://api.github.com/users/octocat")
datos = net.post("https://api.com/datos", {token: key, id: 1})
net.download("https://ejemplo.com/archivo.zip", "local/archivo.zip")
ip    = net.resolve("ejemplo.com")
ping  = net.pulse("ejemplo.com", 443)   -- {alive, latency_ms}
```

#### `env` — Configuración
```orion
puerto = env.pull("PORT", 8080)
modo   = env.pull("MODE", "produccion")
config = env.load(".env")
```

### Utilidades

```orion
use "strings"
use "datetime"
use "random"
use "process"
use "log"
```

#### `strings`
```orion
upper  = strings.upper("hola")
partes = strings.split("a,b,c", ",")
unido  = strings.join(lista, " - ")
ok     = strings.contains(texto, "orion")
b64    = strings.encode_base64(datos)
```

#### `datetime`
```orion
ahora   = datetime.now()
hoy     = datetime.today()
ts      = datetime.timestamp()
partes  = datetime.parts(ahora)   -- {year, month, day, hour, ...}
manana  = datetime.add_days(hoy, 1)
diff    = datetime.diff_days("2026-01-01", "2026-12-31")
dia     = datetime.weekday(hoy)   -- "Thursday"
```

#### `random`
```orion
n    = random.int(1, 100)
elem = random.choice(["rojo", "verde", "azul"])
id   = random.uuidv4()
mix  = random.shuffle([1, 2, 3, 4, 5])
```

#### `process`
```orion
res = process.execute("git status")
show res.out
process.background("servidor.exe")
existe = process.check_dependency("ffmpeg")
```

### Seguridad y criptografía

```orion
use "crypto"
```

```orion
hash    = crypto.sha256("datos sensibles")
token   = crypto.token(32)
id      = crypto.uuid()

-- Hash de contraseñas
h       = crypto.hash(password)
ok      = crypto.verify_hash(password, h)

-- Firma HMAC
firma   = crypto.sign(datos, secreto)
valida  = crypto.verify(datos, firma, secreto)

-- Cifrado simétrico
cifrado = crypto.encrypt(datos, clave)
texto   = crypto.decrypt(cifrado.cipher, cifrado.key)
```

### IA y visión

```orion
use "ai"
use "vision"
use "insight"
```

```orion
-- ai
resumen    = ai.summarize(texto)
categoria  = ai.classify(email, ["spam", "trabajo", "personal"])
codigo     = ai.code("función que ordena lista de diccionarios por fecha")
sentimiento = ai.sentiment(reseña)
traduccion = ai.translate(texto, to: "english")
extraccion = ai.extract(factura, ["numero", "fecha", "total"])

-- vision
info   = vision.info("foto.jpg")       -- {width, height}
vision.resize("foto.jpg", 800, 600, "thumb.jpg")
vision.grayscale("foto.jpg", "gris.jpg")
b64    = vision.to_base64("foto.jpg")

-- insight (documentos con IA)
analisis = insight.analyze("contrato.png", "¿Cuál es la fecha de vencimiento?")
```

### Científicos y simulación

```orion
use "matrix"
use "quantum"
use "cosmos"
```

```orion
-- matrix — álgebra lineal
A   = [[1,2],[3,4]]
B   = matrix.transpose(A)
C   = matrix.mul(A, B)
det = matrix.det(A)
inv = matrix.inverse(A)

-- quantum — simulación cuántica
q   = quantum.qubit()
q2  = quantum.apply(q, quantum.gate_H)
med = quantum.measure(q2, shots: 1000)   -- {0: 512, 1: 488}

-- cosmos — simulación N-cuerpos
u   = cosmos.create(5)
u   = cosmos.run(u, steps: 100)
show cosmos.summary(u)
```

---

## CLI completo

```bash
# Ejecutar
orion archivo.orx

# REPL interactivo
orion

# Nuevo proyecto con scaffold
orion new mi-api

# Verificar sintaxis
orion check main.orx

# Verificar tipos estáticos
orion check --types main.orx

# Hot reload al guardar
orion watch main.orx

# Benchmark
orion bench main.orx --runs=20

# Tests auto-descubrimiento (test_*.orx)
orion test
orion test tests/

# Diagnóstico del entorno
orion doctor
```

### REPL

```
orion> 2 + 3
5
orion> nombre = "Orion"
orion> "Hola " + nombre
"Hola Orion"
orion> fn doble(x) { return x * 2 }
orion> doble(21)
42
orion> :vars     ← muestra variables activas
orion> :fns      ← muestra funciones definidas
orion> :clear    ← limpia el estado
orion> :exit     ← salir
```

### `orion new` genera

```
mi-api/
├── main.orx          ← servidor backend listo
├── orion.json        ← manifiesto del proyecto
├── .env.example
├── .gitignore
├── lib/
│   └── utils.orx
└── test/
    └── test_routes.orx
```

---

## Arquitectura

```
archivo.orx
    │
    ▼
lexer.rs        ← tokenización (UTF-8, interpolación, escapes)
    │
    ▼
parser.rs       ← AST recursivo descendente
    │
    ▼
typechecker.rs  ← verificación de tipos estática (opcional)
    │
    ▼
codegen.rs      ← compilación AST → bytecode
    │
    ▼
vm.rs           ← ejecución (Rust nativo, sin GIL)
```

**Sin Python. Sin runtime externo. Un solo ejecutable.**

---

## Rendimiento

| Escenario | Tiempo | vs Python |
|---|---|---|
| Hola mundo | < 1 ms | ~50x más rápido |
| CSV 15 filas + 6 operaciones regex | ~8 ms | ~30x más rápido |
| Pipeline startup | ~1 ms | Python: 150-400 ms solo de startup |

---

## Extensión VSCode

- Syntax highlighting completo
- IntelliSense (LSP integrado)
- Diagnósticos del compilador en tiempo real
- Code lenses: `▶ Ejecutar` + métricas de complejidad
- Watch mode con output en panel
- Shape diagram visual
- Route explorer + REST client integrado
- Test explorer (descubre `test_*.orx`)
- Import graph
- Debugger DAP
- REPL integrado
- **Binario bundleado** — zero-config, sin instalar nada extra

---

## Estado del runtime

| Componente | Estado | Tecnología |
|---|---|---|
| Lexer + escape sequences | ✅ Completo | Rust |
| Parser | ✅ Completo | Rust |
| Type checker | ✅ Completo | Rust |
| Compilador bytecode | ✅ Completo | Rust |
| VM (ejecución) | ✅ Completo | Rust |
| OOP (shape, act, using, is) | ✅ Completo | Rust |
| Type hints opcionales | ✅ Completo | Rust |
| Manejo de errores (attempt/handle) | ✅ Completo | Rust |
| Async / await | ✅ Completo | Rust |
| REPL interactivo | ✅ Completo | Rust |
| Servidor HTTP nativo | ✅ Completo | Rust |
| IA nativa (think/learn/sense) | ✅ Completo | Rust |
| Errores con span y contexto visual | ✅ Completo | Rust |
| Debugger interactivo (breakpoints, step, watches) | ✅ Completo | Rust |
| DAP — Debug Adapter Protocol (VSCode) | ✅ Completo | Rust |
| LSP — diagnósticos en tiempo real | ✅ Completo | Rust |
| JIT — Cranelift (I/O, módulos, OOP) | ✅ Completo | Cranelift |
| AOT — compilación a ejecutable nativo | ✅ Completo | Cranelift |
| FFI — librerías nativas externas | ✅ Completo | libloading |
| Package manager (add/remove/list/search/publish) | ✅ Completo | Rust |
| Registry oficial en GitHub | ✅ Completo | GitHub API |
| Módulos stdlib | ✅ 35+ módulos | Rust |
| CLI completo | ✅ Completo | Rust |
| Extensión VSCode con binario bundleado | ✅ Completo | TypeScript |

---

## Stdlib completa (35+ módulos)

### Core
`fs` `json` `strings` `datetime` `random` `regex` `env` `process` `crypto`

### Red
`net` `ws` `serve`

### Backend
`db` `auth` `cache` `mail` `validate`

### Automatización
`tarea` `cola` `watch`

### Datos
`csv` `excel` `table` `matrix`

### Utilidades modernas
`template` `formato` `grafo` `pdf`

### Avanzado
`ai` `vision` `insight` `gui` `quantum` `cosmos` `timewarp`

---

## Ecosistema — Roadmap de librerías modernas

> Orion no copia Python. Cada módulo está diseñado para 2025: API simple, rápida y sin configuración.

### Por qué Orion gana a Python aquí

| | Python | Orion |
|---|---|---|
| Velocidad | lento (GIL) | Rust nativo + JIT |
| Arranque | 150-400 ms | < 1 ms |
| AI integrada | pip install | stdlib |
| Compilación nativa | no | `orion --build` |
| Package manager | pip (lento) | `orion --add` (instantáneo) |
| API | legado de los 90s | diseñada en 2025 |

---

### Bloque D — Sistema moderno
*Base de cualquier aplicación real. Sin estas, cualquier app queda incompleta.*

| # | Módulo | Descripción | Crate Rust | Estado |
|---|--------|-------------|------------|--------|
| 1 | `use "zip"` | Comprimir/descomprimir gzip, zip, tar | `flate2` + `zip` | pendiente |
| 2 | `use "secret"` | Leer `.env`, secrets seguros con validación | `dotenvy` | pendiente |
| 3 | `use "log"` | Logging estructurado con niveles, colores y archivos | nativo | pendiente |
| 4 | `use "config"` | Cargar TOML / YAML / JSON como configuración tipada | `toml` + `serde_yaml` | pendiente |
| 5 | `use "crypto2"` | AES-256, RSA, firma de datos, certificados | `aes` + `rsa` | pendiente |
| 6 | `use "stream"` | Pipelines lazy: map, filter, reduce sobre datos infinitos | nativo | pendiente |

---

### Bloque B — Web moderna
*Más allá del `serve` básico: middleware, routing avanzado, protocolos modernos.*

| # | Módulo | Descripción | Crate Rust | Estado |
|---|--------|-------------|------------|--------|
| 7 | `use "router"` | Routing declarativo con parámetros, grupos y middleware | `matchit` | pendiente |
| 8 | `use "middleware"` | Rate limit, CORS, logging, auth en cadena | nativo | pendiente |
| 9 | `use "sse"` | Server-Sent Events para streaming HTTP en tiempo real | nativo | pendiente |
| 10 | `use "proto"` | Serialización binaria rápida (MessagePack) — 10x más rápido que JSON | `rmp-serde` | pendiente |

---

### Bloque C — AI nativa
*La diferenciación más fuerte de Orion. AI de primera clase, sin pip, sin configuración.*

| # | Módulo | Descripción | Crate Rust | Estado |
|---|--------|-------------|------------|--------|
| 11 | `use "llm"` | Llamadas a OpenAI / Anthropic / Ollama / Gemini en 1 línea | `ureq` | pendiente |
| 12 | `use "embed"` | Embeddings de texto, similitud coseno, búsqueda semántica | math nativo | pendiente |
| 13 | `use "vector"` | Base de datos vectorial en memoria con HNSW | `hnsw_rs` | pendiente |

```orion
-- Ejemplo del futuro
use "llm"
use "vector"

respuesta = llm.ask("gpt-4o", "Resume este contrato en 3 puntos: " + contrato)
embedding = llm.embed("text-embedding-3-small", texto)
similares = vector.buscar(db_vectores, embedding, top: 5)
```

---

### Bloque A — Datos modernos
*El reemplazo de pandas/numpy: más rápido, API más simple, en español.*

| # | Módulo | Descripción | Crate Rust | Estado |
|---|--------|-------------|------------|--------|
| 14 | `use "tabla"` v2 | DataFrames columnar, groupby, join, pivot, lazy eval | `polars` | pendiente |
| 15 | `use "serie"` | Series de tiempo, ventanas deslizantes, resample, OHLCV | `polars` | pendiente |
| 16 | `use "stat"` | Estadística moderna: percentil, correlación, regresión lineal | `statrs` | pendiente |
| 17 | `use "num"` | Tensores N-dim, álgebra lineal, FFT, convolución | `ndarray` | pendiente |

```orion
-- Orion vs pandas: mismo resultado, la mitad de líneas
use "tabla"

ventas = tabla.cargar("ventas.csv")
reporte = ventas
    |> tabla.filtrar("region", "Norte")
    |> tabla.agrupar("producto")
    |> tabla.suma("venta")
    |> tabla.ordenar("venta", "desc")
    |> tabla.top(10)
tabla.mostrar(reporte)
```

---

### Bloque E — Cloud native
*Sin pip, sin npm. Cloud como stdlib.*

| # | Módulo | Descripción | Crate Rust | Estado |
|---|--------|-------------|------------|--------|
| 18 | `use "s3"` | Subir/bajar archivos a S3 / R2 / MinIO | `rusty-s3` | pendiente |
| 19 | `use "ssh"` | Ejecutar comandos remotos via SSH, tunnel, SCP | `ssh2` | pendiente |
| 20 | `use "docker"` | Controlar contenedores Docker via API REST | `ureq` | pendiente |

---

### Orden de implementación

```
Bloque D → Bloque B → Bloque C → Bloque A → Bloque E
  (base)     (web)      (AI)      (datos)    (cloud)
```

---

## Contribuir al ecosistema

```bash
# Agregar un módulo a la stdlib
# 1. Crear orion-vm/src/modules/mi_modulo.rs
# 2. Registrar en orion-vm/src/modules/mod.rs
# 3. Agregar dependencia en orion-vm/Cargo.toml

# Publicar un paquete .orx al registry oficial
orion --publish   # requiere orion.json + ORION_GITHUB_TOKEN
```

---

*Orion — construido por Angel Zapata · 2025–2026*
