# Orion Language

Orion es un lenguaje de programación de propósito general con capacidades backend reales.
Sintaxis limpia, tipado opcional, OOP nativa, módulos integrados y pipeline completo en Rust.

> Construido por **Angel Zapata** · 2025–2026

---

## Filosofía

- **Simple** — sin ruido, sin boilerplate. El código se lee como pseudocódigo.
- **Real** — pensado para construir cosas reales, no solo aprender.
- **Moderno** — OOP, type hints, interpolación, módulos, manejo de errores, IA nativa.
- **Rápido** — pipeline completo en Rust: lexer, parser, type checker, codegen y VM.

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

### Servidor HTTP nativo

```orion
use "server" as srv

app = srv.create()

app.get("/", fn(req) {
    return "Hola desde Orion Server!"
})

app.post("/echo", fn(req) {
    return req.body
})

app.get("/saludo/:nombre", fn(req) {
    nombre = req.url_params.nombre
    return "Hola, " + nombre + "!"
})

show "Iniciando servidor en puerto 8080"
app.listen(8080)
```

### Type checker estático

```bash
orion check --types archivo.orx
```

```orion
fn suma(a: int, b: int) -> int {
    return a + b
}

suma("hola", 5)   -- TypeError línea 5: se esperaba int, se recibió string
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
orion-vm/src/lexer.rs      ← tokeniza el código fuente
orion-vm/src/parser.rs     ← genera el AST
orion-vm/src/typechecker.rs← verifica tipos antes de ejecutar
orion-vm/src/codegen.rs    ← compila AST → bytecode
    │
    ▼
orion-vm/src/vm.rs         ← ejecuta el bytecode (VM Rust)
```

**Pipeline completo en Rust** — sin dependencia de Python para ejecutar `.orx`.

**Módulos stdlib:**
```
packages/   → math.orx, strings.orx, list.orx  (escritos en Orion)
stdlib/     → ai, vision, crypto, cosmos, quantum, matrix, insight, timewarp
módulos     → fs, net, json, env, server, datetime, random, process
```

---

## CLI — Orion Command Line

### Instalación

```bash
# Compilar desde fuente
cargo build --release --manifest-path orion-vm/Cargo.toml
```

Luego puedes usar `orion.exe` directamente desde cualquier terminal.

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

# Verificar tipos estáticos
orion check --types main.orx

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

### Diagnóstico

```bash
orion doctor
```

```
┌──────────────────────────┬────────┬──────────────────────────────────┐
│ Componente               │ Estado │ Detalle                          │
├──────────────────────────┼────────┼──────────────────────────────────┤
│ Rust / Cargo             │ ✔ OK   │ cargo 1.89.0                     │
│ Orion VM (orion.exe)     │ ✔ OK   │ release                          │
└──────────────────────────┴────────┴──────────────────────────────────┘
✔ Entorno listo.
```

### REPL interactivo

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
orion> :exit          ← salir
```

**Desde VSCode:** Abre un archivo `.orx` y usa el botón en la barra de herramientas.

---

## Roadmap

### ✅ Fase 1 — Base del lenguaje
- [x] Lexer, Parser, intérprete tree-walker (Python — base inicial)
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

### ✅ Fase 3 — Lenguaje maduro
- [x] **OOP con Shapes** — `shape`, `act`, `using`, `is`, `on_create`
- [x] **Type hints opcionales** — `nombre: string = ""`, `fn f(x: int) -> bool`
- [x] VM Rust con call frames y tabla de funciones
- [x] `attempt/handle` en Rust — pila de handlers, unwinding automático
- [x] `for..in` sobre listas en bytecode
- [x] **REPL interactivo** — multi-línea, auto-print, comandos `:vars`, `:fns`, `:clear`
- [x] Benchmark: intérprete Python vs VM Rust (~137x más rápido)

### ✅ Fase 4 — IA nativa
- [x] Módulo `ai` conectado a APIs reales (Claude / OpenAI) vía `.env`
- [x] `ai.ask()`, `ai.summarize()`, `ai.classify()`, `ai.extract()`, `ai.code()`
- [x] `ai.fix()`, `ai.translate()`, `ai.sentiment()`, `ai.improve()`, `ai.explain()`
- [x] `ai.chat()` — sesión con historial y contexto persistente
- [x] **Sintaxis nativa `think`** — statement directo sin `use ai`
- [x] `learn` y `sense` como opcodes reales en VM Rust
- [x] `vision.describe(img, prompt?)` — Claude Vision o GPT-4o
- [x] `insight.analyze(img, question?)` — análisis de documentos con IA
- [x] **`net.reach()`** — HTTP sin dependencias externas

### ✅ Fase 4B — CLI y tooling moderno
- [x] **CLI completo** — `orion new`, `build`, `check`, `watch`, `bench`, `test`, `doctor`
- [x] **`orion watch`** — hot reload sin Ctrl+C
- [x] **`orion bench`** — benchmark con tabla y barra visual
- [x] **`orion test`** — test runner auto-descubrimiento
- [x] **`orion doctor`** — health check del entorno
- [x] **Gestor de paquetes** — `orion add`, `remove`, `list`, `search`, `update`
- [x] **`async/await`** — concurrencia nativa con OS threads reales

### ✅ Fase 5A — Servidor HTTP nativo
- [x] Módulo `server` con routing por método y path
- [x] Callbacks `fn(req)` con `req.body`, `req.url_params`, `req.headers`
- [x] `app.get()`, `app.post()`, `app.listen(port)`
- [x] Respuestas con `content_type`, `status`, `body`
- [x] Parámetros de ruta dinámica (`:nombre`)

### ✅ Fase 5B — Type checker estático
- [x] `orion check --types archivo.orx`
- [x] Inferencia de tipos en asignaciones
- [x] Verificación de tipos en llamadas a funciones con type hints
- [x] Verificación de tipo de retorno declarado vs. real
- [x] Reporte de errores de tipo con número de línea
- [x] Modo estricto opcional — código sin hints sigue funcionando

### ✅ Fase 5C — Pipeline completo en Rust
- [x] `orion-vm/src/lexer.rs` — tokenizador completo en Rust
- [x] `orion-vm/src/parser.rs` — parser con AST completo en Rust
- [x] `orion-vm/src/typechecker.rs` — type checker en Rust
- [x] `orion-vm/src/codegen.rs` — compilador bytecode en Rust
- [x] Eliminada dependencia de Python para ejecutar `.orx`
- [x] Módulos stdlib portados a Rust (`stdlib_bridge.rs`)

### ✅ Fase 5D — Binario único distribuible
- [x] `orion.exe` sin Python, sin pip, sin dependencias externas
- [x] Compilación con `cargo build --release`

---

### 🔜 Fase 6A — Compilación nativa (Cranelift)

`.orx` compilado directamente a binario nativo sin VM en medio.

```
.orx → Rust pipeline → Cranelift IR → binario nativo
```

- [ ] Backend Cranelift integrado en `orion-vm`
- [ ] `orion compile archivo.orx -o salida` — produce ejecutable nativo
- [ ] Rendimiento objetivo: comparable a Go para scripts típicos

| Pipeline | Velocidad estimada |
|---|---|
| Hoy: Rust pipeline + Rust VM | ~400x vs Python puro |
| Fase 6A: compilación nativa | ~1000x+ estimado |

---

### 🔜 Fase 6B — Ecosistema y comunidad

- [ ] Sitio de documentación oficial (orionlang.dev)
- [ ] Registro de paquetes online — `orion publish` / `orion add <paquete>`
- [ ] Showcase de proyectos reales construidos con Orion
- [ ] Discord / comunidad
- [ ] Guía de contribución y roadmap público

---

## Estado actual de componentes

| Componente | Estado | Tecnología |
|---|---|---|
| Lexer | ✅ Completo | Rust |
| Parser | ✅ Completo | Rust |
| Type checker | ✅ Completo | Rust |
| Compilador bytecode (codegen) | ✅ Completo | Rust |
| VM (ejecución) | ✅ Funcional | Rust |
| OOP (shape, act, using, is) | ✅ Completo | Rust |
| Type hints opcionales | ✅ Completo | Rust |
| Sistema de módulos | ✅ Completo | Rust |
| Módulos stdlib | ✅ 15+ módulos | Rust |
| Servidor HTTP | ✅ Completo | Rust |
| IA nativa (think, learn, sense) | ✅ Completo | Rust |
| `async / await` concurrencia | ✅ Completo | Rust |
| Manejo de errores (attempt/handle) | ✅ Completo | Rust |
| REPL interactivo | ✅ Completo | Rust |
| CLI (new/build/check/watch/bench/test/doctor) | ✅ Completo | Rust |
| Extensión VSCode | ✅ Completa | JavaScript |
| Binario distribuible sin dependencias | ✅ Completo | Rust |

---

## Categoría del lenguaje

Orion es un **lenguaje compilado a bytecode con VM nativa en Rust**.

```
.orx → Rust lexer → Rust parser → type checker → codegen → VM Rust
```

Sin Python. Sin runtime externo. Un solo ejecutable.

---

*Orion — construido por Angel Zapata · 2025–2026*
