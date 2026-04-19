# Orion Language

Orion es un lenguaje de programación diseñado para ser simple, rápido y moderno.
Sintaxis limpia inspirada en Python y Rust, compilado a bytecode y ejecutado por una VM en Rust.

---

## Filosofía

- **Simple** — sin ruido, sin boilerplate. El código se lee como pseudocódigo.
- **Rápido** — VM compilada en Rust. No un intérprete Python lento.
- **Moderno** — funciones de primera clase, interpolación de strings, tipos opcionales.
- **Accesible** — cualquiera puede aprenderlo en minutos.

---

## Sintaxis actual

```orion
-- Variables
nombre = "Orion"
edad   = 25
activo = yes

-- Constantes
const PI = 3.14159

-- Mostrar valores
show(nombre)
show("Hola " + nombre)

-- Condicionales
if edad >= 18 {
  show("Mayor de edad")
} else {
  show("Menor de edad")
}

-- Bucle while
i = 0
while i < 5 {
  show(i)
  i = i + 1
}

-- Bucle for
for x in 1..10 {
  show(x)
}

-- Funciones
fn saludar(nombre) {
  return "Hola " + nombre
}

show(saludar("Orion"))

-- Funciones recursivas
fn factorial(n) {
  if n <= 1 {
    return 1
  }
  return n * factorial(n - 1)
}

show(factorial(10))

-- Listas y diccionarios
numeros = [1, 2, 3, 4, 5]
persona = {"nombre": "Gabriel", "edad": 25}
```

**Tipos:** `int`, `float`, `string`, `bool` (`yes`/`no`), `null`, `list`, `dict`  
**Comentarios:** `-- comentario`  
**Bloques:** `{ }` con indentación opcional

---

## Arquitectura

```
archivo.orx
    │
    ▼
core/lexer.py       ← tokeniza el código fuente
core/parser.py      ← genera el AST
    │
    ▼
compiler/bytecode_compiler.py   ← compila AST → instrucciones JSON
    │
    ▼
archivo.orbc        ← bytecode (JSON)
    │
    ▼
orion-vm/           ← VM en Rust con call frames
    └── target/release/orion.exe
```

---

## Cómo ejecutar

**Desde el CLI:**
```bash
python orion/cli.py archivo.orx
```

**Manual (dos pasos):**
```bash
python compiler/bytecode_compiler.py archivo.orx archivo.orbc
.\orion-vm\target\release\orion.exe archivo.orbc
```

**Desde VSCode:**  
Abre un archivo `.orx` y pulsa el botón `⚡ Orion` en la barra de estado.

---

## Roadmap

### ✅ Fase 1 — Prototipo (2025)
- [x] Intérprete Python funcional
- [x] Sintaxis básica: variables, if/else, while, for, funciones
- [x] Extensión VSCode con resaltado de sintaxis
- [x] CLI oficial

### ✅ Fase 2 — VM Rust (2025–2026)
- [x] Compilador de bytecode (Python → `.orbc`)
- [x] VM en Rust con stack de valores
- [x] Call frames para funciones de usuario
- [x] Tabla de funciones separada del main
- [x] Integración extensión VSCode → compilador → VM Rust

### 🔄 Fase 3 — Lenguaje robusto (2026)
- [ ] Errores con número de línea
- [ ] String interpolation: `"Hola ${nombre}"`
- [ ] Builtins: `input()`, `range()`, `len()`, métodos de string
- [ ] Manejo de errores: `attempt / handle`
- [ ] Funciones como valores (first-class)
- [ ] Closures

### 📋 Fase 4 — Ecosistema (2026–2027)
- [ ] Sistema de módulos: `use math`
- [ ] Clases y OOP: `class Persona { }`
- [ ] Tipado opcional: `fn suma(a: int, b: int) -> int`
- [ ] Concurrencia: `async / await`
- [ ] Gestor de paquetes: `orion add paquete`
- [ ] Lexer y parser reescritos en Rust (eliminar dependencia de Python)

### 🚀 Fase 5 — Producción (2027+)
- [ ] Compilación a binario nativo
- [ ] Librerías estándar (archivos, red, JSON, HTTP)
- [ ] Comunidad: documentación, foros, Discord
- [ ] Frameworks: web, IA, bases de datos

---

## Estado actual

| Componente | Estado |
|---|---|
| Lexer | ✅ Python |
| Parser | ✅ Python |
| Compilador bytecode | ✅ Python |
| VM (ejecutor) | ✅ Rust |
| Extensión VSCode | ✅ Activa |
| CLI | ✅ Funcional |
| Errores con línea | ⏳ Próximo |
| String interpolation | ⏳ Próximo |

---

*Orion — construido por Angel Zapata · 2025–2026*
