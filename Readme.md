# Orion Language

Orion es un lenguaje de programación de propósito general diseñado para uso real.
Sintaxis limpia, tipado opcional, OOP nativa, módulos integrados y arquitectura de VM.

> Construido por **Angel Zapata** · 2025–2026

---

## Filosofía

- **Simple** — sin ruido, sin boilerplate. El código se lee como pseudocódigo.
- **Real** — pensado para construir cosas reales, no solo aprender.
- **Moderno** — OOP, type hints, interpolación, módulos, manejo de errores.
- **Extensible** — arquitectura de VM lista para compilar a nativo en el futuro.

---

## Sintaxis

### Fundamentos

```orion
-- Variables
nombre = "Orion"
edad   = 25
activo = yes

-- Constantes
const PI = 3.14159

-- Variables con type hint (opcional)
ciudad:  string = "Monterrey"
version: int    = 1

-- Mostrar valores
show nombre
show "Hola " + nombre
show "Versión ${version} de ${nombre}"   -- interpolación
```

### Control de flujo

```orion
-- if / elsif / else
if edad >= 18 {
    show "Mayor de edad"
} elsif edad >= 13 {
    show "Adolescente"
} else {
    show "Menor de edad"
}

-- while
i = 0
while i < 5 {
    show i
    i = i + 1
}

-- for en rango
for x in 1..10 {
    show x
}

-- for en colección
nombres = ["Ana", "Luis", "Eva"]
for n in nombres {
    show n
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

fn es_par(n: int) -> bool {
    return n % 2 == 0
}

show saludar("Orion")
show str(suma(10, 20))
```

### OOP — Shapes

```orion
-- Shape básico
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
        edad = edad + 1
    }
}

p = Persona("Gabriel", 25)
p.saludar()
p.cumpleanos()
show str(p.edad)    -- 26

-- Verificación de tipo
if p is Persona {
    show "Es una Persona"
}

-- Composición con using
shape Animal {
    nombre: string = ""
    sonido: string = ""

    act hablar() {
        show nombre + " dice: " + sonido
    }
}

shape Perro {
    using Animal
    raza: string = ""

    on_create(n: string, s: string, r: string) {
        nombre = n
        sonido = s
        raza   = r
    }

    act buscar() {
        show nombre + " busca la pelota!"
    }
}

d = Perro("Rex", "Guau", "Labrador")
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

### IA nativa — `think`

```orion
-- Sin módulos, sin imports: IA directa como statement
think "Resume el siguiente texto en 3 puntos: " + contenido

-- Equivalente a ai.ask(), pero nativo al lenguaje
think "¿Cuál es la capital de Francia?"
```

### Módulos

```orion
use "packages/math"          as m
use "packages/strings"       as s
use "packages/list"          as lst
use "json"                   as j
use "packages/math" take [sqrt, pow]

show m.sqrt(25)        -- 5.0
show s.reverse("hola") -- aloh
show lst.sort([3,1,2]) -- [1, 2, 3]
```

### Módulos avanzados integrados

```orion
-- IA
use "ai" as ai
respuesta = ai.ask("Resume este texto: " + contenido)

-- Red
use "net" as net
data = net.reach("https://api.ejemplo.com/datos")

-- Archivos
use "fs" as fs
contenido = fs.load("archivo.txt")
fs.safe_write("salida.txt", contenido)

-- Encriptación
use "crypto" as crypto
hash = crypto.sha256("texto secreto")
```

---

## Tipos de datos

| Tipo | Ejemplo | Literal |
|---|---|---|
| `int` | `42` | entero |
| `float` | `3.14` | decimal |
| `string` | `"Hola"` | texto entre comillas |
| `bool` | `yes` / `no` | booleano |
| `null` | `null` | nulo |
| `list` | `[1, 2, 3]` | lista |
| `dict` | `{"k": "v"}` | diccionario |
| shape instance | `Persona("Ana", 30)` | objeto |

---

## Arquitectura

```
archivo.orx  (código fuente)
    │
    ▼
core/lexer.py          ← tokeniza el código fuente
core/parser.py         ← genera el AST
    │
    ├──▶  core/eval.py          ← intérprete directo (tree-walker, usado hoy)
    │
    ▼
compiler/bytecode_compiler.py   ← compila AST → instrucciones JSON
    │
    ▼
archivo.orbc  (bytecode JSON)
    │
    ▼
orion-vm/src/  ← VM en Rust (en desarrollo para Fase 3C)
```

**Módulos stdlib:**
```
modules/    → fs, net, json, env, strings, datetime, random, show, process
lib/        → io, math, sys, collections
stdlib/     → ai, vision, crypto, cosmos, quantum, matrix, insight, timewarp
packages/   → math.orx, strings.orx, list.orx  (escritos en Orion)
```

---

## CLI — Orion Command Line

Orion incluye un CLI completo inspirado en las mejores herramientas modernas (Rust/Cargo, Node/npm, Flutter).

### Instalación

```bash
pip install -e .
```

Luego puedes usar `orion` directamente desde cualquier terminal.

### Comandos de desarrollo

```bash
# Ejecutar un archivo directamente
orion archivo.orx

# REPL interactivo (multi-línea, auto-print, comandos)
orion
```

### Comandos de proyecto

```bash
# Crear un proyecto backend nuevo (scaffold completo)
orion new mi-api

# Verificar sintaxis sin ejecutar
orion check main.orx

# Compilar a bytecode sin ejecutar
orion build main.orx
```

`orion new` genera esta estructura automáticamente:

```
mi-api/
├── main.orx          ← servidor backend listo para usar
├── orion.json        ← manifiesto del proyecto
├── .env.example      ← variables de entorno
├── .gitignore
├── lib/
│   └── utils.orx     ← helpers reutilizables
└── test/
    └── test_routes.orx
```

### Comandos de calidad

```bash
# Hot reload — recompila y re-ejecuta automáticamente al guardar
orion watch main.orx

# Benchmark de ejecución (default: 10 corridas)
orion bench main.orx
orion bench main.orx --runs=20

# Test runner — auto-descubre test_*.orx y test/*.orx
orion test
orion test mi-carpeta/
```

#### `orion watch` — desarrollo sin interrupciones

Detecta cambios al guardar y recompila + re-ejecuta automáticamente. Sin `Ctrl+C`, sin reinicios manuales.

```
──────────────────── 22:14:01 ────────────────────
Hola mundo
──────────────────── 22:14:08 ────────────────────   ← guardaste el archivo
Hola Orion
```

#### `orion bench` — rendimiento real

Mide tiempo de ejecución con N corridas y muestra tabla con promedio, mediana, mejor, peor y std dev, con barra visual proporcional.

#### `orion test` — testing automatizado

Descubre todos los archivos de test, los compila, ejecuta y reporta:

```
┌─────────────────────────────┬────────────┬──────────┬──────────────┐
│ Archivo                     │ Estado     │ Tiempo   │ Info         │
├─────────────────────────────┼────────────┼──────────┼──────────────┤
│ test/test_routes.orx        │ ✔ PASS     │ 133ms    │              │
│ test/test_auth.orx          │ ✖ FAIL     │ 45ms     │ Línea 7: ... │
└─────────────────────────────┴────────────┴──────────┴──────────────┘
✔ 1/2 tests pasaron.
```

### Gestión de paquetes

```bash
orion add math          # instalar paquete
orion remove math       # desinstalar
orion list              # paquetes instalados
orion search json       # buscar en el registro
orion update            # actualizar todos
```

### Diagnóstico

```bash
# Verifica todo el entorno: Python, Rust, VM, dependencias pip
orion doctor
```

```
┌──────────────────────────┬────────┬──────────────────────────────────┐
│ Componente               │ Estado │ Detalle                          │
├──────────────────────────┼────────┼──────────────────────────────────┤
│ Python >= 3.10           │ ✔ OK   │ 3.13.2                           │
│ Rust / Cargo             │ ✔ OK   │ cargo 1.89.0                     │
│ Orion VM (orion.exe)     │ ✔ OK   │ release                          │
│ pip: rich                │ ✔ OK   │ instalado                        │
│ pip: watchdog            │ ✔ OK   │ instalado                        │
│ Bytecode compiler        │ ✔ OK   │ compiler/bytecode_compiler.py    │
└──────────────────────────┴────────┴──────────────────────────────────┘
✔ Entorno listo.
```

### REPL interactivo

```bash
orion
```

```
orion> 2 + 3
5
orion> nombre = "Orion"
orion> "Hola " + nombre
"Hola Orion"
orion> fn doble(x) { return x * 2 }
orion> doble(21)
42
orion> :vars          ← muestra todas las variables
orion> :fns           ← muestra todas las funciones
orion> :clear         ← limpia el estado
orion> :help          ← ayuda
orion> :exit          ← salir
```

Soporta bloques multi-línea:

```
orion> fn factorial(n) {
  ...>     if n <= 1 { return 1 }
  ...>     return n * factorial(n - 1)
  ... > }
orion> factorial(10)
3628800
```

**Desde VSCode:** Abre un archivo `.orx` y usa el botón `⚡ Orion` en la barra de estado.

---

## Roadmap

### ✅ Fase 1 — Base del lenguaje
- [x] Lexer, Parser, intérprete tree-walker (Python)
- [x] Variables, constantes, if/elsif/else, while, for
- [x] Funciones con return, recursión
- [x] Listas, diccionarios, indexación
- [x] CLI oficial (`orion archivo.orx`)
- [x] Extensión VSCode con syntax highlighting

### ✅ Fase 2 — Lenguaje funcional completo
- [x] String interpolation `"Hola ${nombre}"`
- [x] Sistema de módulos (`use "modulo" as m`)
- [x] Imports selectivos (`use "mod" take [fn1, fn2]`)
- [x] Módulos stdlib: `fs`, `net`, `json`, `crypto`, `ai`, `vision`, `math`, `strings`, `list`
- [x] Manejo de errores (`attempt / handle`)
- [x] Match expression
- [x] Lambdas y closures
- [x] Operadores compuestos (`+=`, `-=`, `*=`, `/=`)
- [x] For..in sobre colecciones
- [x] Compilador de bytecode Python → `.orbc`
- [x] VM en Rust con call frames y tabla de funciones

### ✅ Fase 3 — Lenguaje maduro
- [x] **OOP con Shapes** — `shape`, `act`, `using`, `is`, `on_create`
- [x] **Type hints opcionales** — `nombre: string = ""`, `fn f(x: int) -> bool`
- [x] Pruebas exhaustivas de OOP y type hints
- [x] Bytecode compiler actualizado para shapes, acts, OOP
- [x] Language server VSCode con syntax highlighting de `shape`, `act`, `using`, `is`
- [x] Snippets VSCode para OOP

### ✅ Fase 3C — VM Rust completa
- [x] VM Rust soporta `DefineShape`, `CallMethod`, `GetAttr`, `SetAttr`, `IsInstance`
- [x] Instanciación de shapes, herencia `using`, `on_create` en Rust
- [x] **`attempt/handle`** en Rust — opcode `BeginAttempt`/`EndAttempt`, pila de handlers, unwinding automático
- [x] **`for..in` sobre listas** en bytecode — bucle con índice compilado, sin opcode extra
- [x] **REPL interactivo** — `orion` sin args abre shell interactivo
- [x] Benchmark: intérprete Python vs VM Rust (~137x más rápido)

### ✅ Fase 4 — IA nativa
- [x] Módulo `ai` conectado a APIs reales (Claude / OpenAI) vía `.env`
- [x] `ai.ask()`, `ai.summarize()`, `ai.classify()`, `ai.extract()`, `ai.code()`
- [x] `ai.fix()`, `ai.translate()`, `ai.sentiment()`, `ai.improve()`, `ai.explain()`
- [x] `ai.qa()`, `ai.complete()`, `ai.search_in()`
- [x] `ai.chat()` — sesión con historial y contexto persistente
- [x] Auto-detección de proveedor: Claude primero, OpenAI como fallback
- [x] **Sintaxis nativa `think`** — statement directo sin `use ai`, funciona en intérprete y bytecode
- [x] **`net.reach()` real** — HTTP sin dependencias externas (usa `urllib`), `requests` como upgrade opcional

### ✅ Fase 4B — CLI y tooling moderno
- [x] **CLI completo** — `orion new`, `build`, `check`, `watch`, `bench`, `test`, `doctor`
- [x] **`orion new`** — scaffold de proyecto backend (main.orx, orion.json, .gitignore, lib/, test/)
- [x] **`orion watch`** — hot reload, recompila y re-ejecuta al guardar (sin Ctrl+C)
- [x] **`orion bench`** — benchmark con N corridas, tabla con barra visual proporcional
- [x] **`orion test`** — test runner que auto-descubre test_*.orx y test/*.orx
- [x] **`orion doctor`** — health check completo del entorno de desarrollo
- [x] **`orion build`** — compila a .orbc sin ejecutar
- [x] **`orion check`** — verifica sintaxis y reporta nodos en ms
- [x] **Gestor de paquetes** — `orion add`, `remove`, `list`, `search`, `update`
- [x] **`async/await`** — concurrencia nativa con OS threads reales
- [x] **REPL v2** — multi-línea, auto-print de expresiones, comandos `:vars`, `:fns`, `:clear`

---

### ✅ Fase 4C — IA nativa completa

- [x] `learn` y `sense` como opcodes reales en VM Rust (`AiLearn` / `AiSense`)
- [x] `AiAsk` como instrucción nativa en Rust (HTTP directo a Anthropic/OpenAI via `ureq`, sin Python)
- [x] `vision.describe(img, prompt?)` — Claude Vision o GPT-4o describe cualquier imagen
- [x] `vision.load_url(url)` — carga imagen desde URL sin dependencias externas
- [x] `insight.analyze(img, question?)` — análisis de documentos con IA + análisis estructural combinados
- [x] `insight._load_image` reescrito con `urllib` (sin `requests`)

```orion
-- IA como parte del lenguaje, sin imports
learn "eres un asistente de código"
sense datos_del_usuario
think "resume esto en 3 puntos: " + contenido

-- Vision con IA real
use "vision" as v
img = v.load("factura.jpg")
descripcion = v.describe(img, "¿Qué campos tiene esta factura?")

-- Insight con IA real
use "insight" as ins
analisis = ins.analyze(img, "¿Hay firmas y sellos en este documento?")
```

---

### 🔜 Fase 5A — Servidor HTTP nativo

Sintaxis nativa para construir APIs sin frameworks externos.

```orion
use "server" as srv

srv.on(8080) {
    GET "/" {
        return "Hola desde Orion"
    }
    POST "/echo" {
        body = request.json()
        return body
    }
}
```

- [ ] Módulo `server` en stdlib con routing declarativo
- [ ] Objetos `request` y `response` nativos
- [ ] Soporte para middlewares con lambdas
- [ ] `orion new --api` — scaffold de proyecto con servidor listo

---

### 🔜 Fase 5B — Type checker estático

El salto más importante en madurez percibida del lenguaje.

```bash
orion check --types archivo.orx   # verifica tipos antes de ejecutar
```

- [ ] Inferencia de tipos en asignaciones (`x = 5` → `x: int` implícito)
- [ ] Verificación de tipos en llamadas a funciones con type hints declarados
- [ ] Verificación de tipo de retorno declarado vs. real
- [ ] Reporte de errores de tipo con número de línea antes de ejecutar
- [ ] Modo estricto opcional — el código sin hints sigue funcionando igual

```orion
fn suma(a: int, b: int) -> int {
    return a + b
}

suma("hola", 5)   -- TypeError en línea 6: se esperaba int, se recibió string
```

---

### 🔜 Fase 5C — Lexer y Parser en Rust

Eliminar Python del pipeline de ejecución para producir un binario único sin dependencias.

```
HOY:   Python lexer → Python parser → bytecode → Rust VM
META:  Rust lexer  → Rust parser  → bytecode → Rust VM
```

- [ ] `orion-vm/src/lexer.rs` — tokenizador completo en Rust
- [ ] `orion-vm/src/parser.rs` — parser con el mismo AST que el actual
- [ ] Eliminar dependencia de Python para ejecutar `.orx`
- [ ] Tiempo de startup: de ~200ms a <5ms

---

### 🔜 Fase 5D — Binario único distribuible

Con Lexer+Parser en Rust, Orion se distribuye como un solo ejecutable sin dependencias.

- [ ] `orion.exe` / `orion` sin Python, sin pip, sin Cargo en la máquina del usuario
- [ ] Instalación: descarga el binario y ejecuta
- [ ] `orion build` produce `.orbc` portable entre máquinas
- [ ] Cross-compilation para Linux, macOS y Windows desde CI

```bash
# Instalación futura
curl -fsSL https://orionlang.dev/install.sh | sh
```

---

### 🚀 Fase 6A — Compilación nativa (Cranelift)

`.orx` compilado directamente a binario nativo sin VM en medio.

```
.orx → Rust pipeline → Cranelift IR → binario nativo
```

- [ ] Backend Cranelift integrado en `orion-vm`
- [ ] `orion compile archivo.orx -o salida` — produce ejecutable nativo
- [ ] Rendimiento objetivo: comparable a Go para scripts típicos
- [ ] Cranelift primero; LLVM como backend opcional de alto rendimiento

| Pipeline | Velocidad estimada |
|---|---|
| Hoy: Python + Rust VM | ~137x vs Python puro |
| Fase 5D: Rust puro + Rust VM | ~400x estimado |
| Fase 6A: compilación nativa | ~1000x+ estimado |

---

### 🚀 Fase 6B — Ecosistema y comunidad *(paralelo a 6A)*

- [ ] Sitio de documentación oficial (orionlang.dev)
- [ ] Registro de paquetes online — `orion publish` / `orion add <paquete>`
- [ ] Showcase de proyectos reales construidos con Orion
- [ ] Discord / comunidad
- [ ] Guía de contribución y roadmap público

---

## Estado actual de componentes

| Componente | Estado | Tecnología |
|---|---|---|
| Lexer | ✅ Completo | Python |
| Parser | ✅ Completo | Python |
| Intérprete (tree-walker) | ✅ Completo | Python |
| Compilador bytecode | ✅ Completo + OOP | Python |
| VM Rust (funciones básicas) | ✅ Funcional | Rust |
| VM Rust (OOP / shapes) | 🔄 En progreso | Rust |
| OOP (shape, act, using, is) | ✅ Completo | — |
| Type hints opcionales | ✅ Completo | — |
| Sistema de módulos | ✅ Completo | Python |
| Módulos stdlib | ✅ 15+ módulos | Python |
| Extensión VSCode | ✅ Completa + OOP | TypeScript |
| CLI (`orion new/build/check/watch/bench/test/doctor`) | ✅ Completo | Python |
| Gestor de paquetes (`orion add/remove/list`) | ✅ Completo | Python |
| REPL interactivo v2 (multi-línea, auto-print) | ✅ Completo | Python |
| `async / await` concurrencia nativa | ✅ Completo | Python + threads |
| Manejo de errores | ✅ attempt/handle | Python |
| String interpolation | ✅ Completa | Python |

---

## Categoría del lenguaje

Orion es un **lenguaje interpretado con compilación a bytecode**, en la misma categoría que Python y Lua.

```
Hoy:     .orx → intérprete Python (tree-walker)
Fase 3C: .orx → .orbc → VM Rust  (~10-50x más rápido)
Futuro:  .orx → .orbc → binario nativo (comparable a Go)
```

---

*Orion — construido por Angel Zapata · 2025–2026*
