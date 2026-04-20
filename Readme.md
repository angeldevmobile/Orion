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

## Cómo ejecutar

```bash
# REPL interactivo (sin argumentos)
python src/main.py

# Intérprete directo
python src/main.py archivo.orx

# Compilar a bytecode
python compiler/bytecode_compiler.py archivo.orx

# Compilar + ejecutar con VM Rust
python compiler/bytecode_compiler.py archivo.orx
.\orion-vm\target\release\orion.exe archivo.orbc
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
- [x] **REPL interactivo** — `python src/main.py` sin args abre shell interactivo
- [x] Benchmark: intérprete Python vs VM Rust (~137x más rápido)

### 🔄 Fase 4 — IA nativa *(en progreso)*
- [x] Módulo `ai` conectado a APIs reales (Claude / OpenAI) vía `.env`
- [x] `ai.ask()`, `ai.summarize()`, `ai.classify()`, `ai.extract()`, `ai.code()`
- [x] `ai.fix()`, `ai.translate()`, `ai.sentiment()`, `ai.improve()`, `ai.explain()`
- [x] `ai.qa()`, `ai.complete()`, `ai.search_in()`
- [x] `ai.chat()` — sesión con historial y contexto persistente
- [x] Auto-detección de proveedor: Claude primero, OpenAI como fallback
- [x] **Sintaxis nativa `think`** — statement directo sin `use ai`, funciona en intérprete y bytecode
- [x] **`net.reach()` real** — HTTP sin dependencias externas (usa `urllib`), `requests` como upgrade opcional
- [ ] `learn` y `sense` como statements nativos
- [ ] Módulos `vision` e `insight` con APIs reales
- [ ] `AiAsk` como instrucción nativa en VM Rust (HTTP directo en Rust sin Python)

### 🚀 Fase 5 — Producción y comunidad
- [ ] Lexer y Parser reescritos en Rust
- [ ] Compilación a binario nativo (LLVM o Cranelift)
- [ ] Gestor de paquetes: `orion add paquete`
- [ ] Documentación oficial
- [ ] Concurrencia: `async / await`
- [ ] Comunidad: foro, Discord, ejemplos

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
| CLI | ✅ Funcional | Python |
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
