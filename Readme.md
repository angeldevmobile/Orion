# Orion Language

Orion is a programming language for backend work and automation.
Clean syntax, optional typing, native OOP, 58 built-in modules and a full pipeline written in Rust.

> Built by **Angel Zapata** · 2025-2026

> **Note on naming.** Orion was designed by a Spanish-speaking developer. Core
> keywords are English (`fn`, `return`, `if`, `while`, `shape`, `serve`), while
> parts of the standard library use Spanish names, often with an English alias:
> `db.insertar` / `db.insert`, `cache.tamaño` / `cache.len`. Examples below use
> the real names, so they run as written.

---

## Demo

![Demo - Terminal](assets/demo_terminal.jpeg)

```orion
-- demo/demo_ventas_q1.orx  -  70 lines · 16 ms
use "excel" as excel

full_data = excel.cruzar(sellers, budgets, "region", "left")
pivot     = excel.pivot(full_data, "region", "producto", "venta")

excel.write_multi("sales_report.xlsx", {
    "Summary":   summary,
    "By Region": by_region,
    "Top 10":    top_10,
    "Pivot":     pivot
})
```

```
╔══════════════════════════════════════════════╗
║   Q1 2026 Results                            ║
╠══════════════════════════════════════════════╣
║  Total sellers        : 20                   ║
║  Total sales          : USD 1487000          ║
║  Overall attainment   : 100.2%               ║
║  Largest sale         : USD 110000           ║
╠══════════════════════════════════════════════╣
║  → demo/reporte_analisis.xlsx  (5 sheets)    ║
║  → demo/reporte_detalle.xlsx   (styled)      ║
╚══════════════════════════════════════════════╝
[Orion] 15.978 ms
```

![Demo - Excel Output](assets/demo_excel.jpeg)

---

## Philosophy

- **No boilerplate** - code reads like pseudocode. One task, five lines at most.
- **Built for real work** - APIs, automation, data pipelines.
- **Modern** - OOP, type hints, string interpolation, async/await, regex, and AI as a language keyword.
- **Fast** - the whole pipeline is Rust: lexer → parser → type checker → codegen → VM. Loading and aggregating 500k CSV rows is **2× faster than Python at the same memory** ([reproducible benchmark](bench/)).
- **Safe** - parameterized queries, validation at the boundary, native crypto.

---

## Installation

### Prebuilt binary (recommended)

Download the executable for your platform from
[the latest release](https://github.com/angeldevmobile/Orion/releases/latest).
It is a single file, with no runtime and no dependencies.

| Platform | File |
|---|---|
| Windows x64 | `orion-win32-x64.exe` |
| Linux x64 | `orion-linux-x64` |
| macOS Apple Silicon | `orion-darwin-arm64` |

```bash
# Linux / macOS - rename, make executable, put it on the PATH
chmod +x orion-linux-x64
sudo mv orion-linux-x64 /usr/local/bin/orion

orion file.orx
```

On Windows, rename the `.exe` to `orion.exe` and add it to your `PATH`.

### VS Code extension

Install it from the Marketplace:
[**Orion Language**](https://marketplace.visualstudio.com/items?itemName=AngelZapata.oriondev).

The extension downloads the compiler the first time you open a `.orx` file,
taking it from the latest release and storing it in VS Code's global storage.
If `orion` is already on your `PATH`, it uses that one instead.

### Build from source

```bash
cargo build --release --manifest-path orion-vm/Cargo.toml
./orion-vm/target/release/orion file.orx
```

---

## Quick start

Create a file called `hello.orx` and run it:

```orion
name    = "Orion"
version = 1

show "Hello from ${name} v${version}"

-- Ranges are half-open: 1..5 covers 1, 2, 3 and 4.
for i in 1..5 {
    show "  line ${i}"
}
```

```bash
orion hello.orx
```

```
Hello from Orion v1
  line 1
  line 2
  line 3
  line 4
[Orion] 1.346 ms
```

Or run the full demo:

```bash
orion demo/demo_ventas_q1.orx
```

---

## Syntax

### Variables and types

```orion
-- Variables
name   = "Orion"
age    = 25
active = yes

-- Constants
const PI = 3.14159

-- Optional type hints
city:    string = "Monterrey"
version: int    = 1

-- Printing values
show name
show "Hello " + name
show "Version ${version} of ${name}"   -- interpolation

-- Escape sequences
path    = "C:\\users\\documents"
line    = "name\tsurname\nage"
pattern = "\\d{4}-\\d{2}-\\d{2}"       -- regex: \d{4}-\d{2}-\d{2}
```

### Data types

| Type | Example | Description |
|---|---|---|
| `int` | `42`, `0xFF`, `0b1010` | 64-bit integer, hex and binary literals |
| `float` | `3.14`, `1.5e-3` | Decimal, scientific notation |
| `string` | `"hi"`, `r"raw"`, `"""multi"""` | Text with `${var}` interpolation |
| `bool` | `yes` / `no` | Boolean |
| `list` | `[1, 2, 3]` | Dynamic array |
| `dict` | `{"k": "v"}` | Hash map |
| `null` | `null` | Explicit null |
| shape | `Person("Ana", 30)` | Shape instance (object) |

### Control flow

```orion
-- if / elsif / else
if age >= 18 {
    show "Adult"
} elsif age >= 13 {
    show "Teenager"
} else {
    show "Child"
}

-- while
i = 0
while i < 5 {
    show i
    i += 1
}

-- for over a range — half-open: 1..10 covers 1 through 9
for x in 1..10 { show x }

-- for over a collection
for n in ["Ana", "Luis", "Eva"] { show n }

-- match
result = match value {
    1    => "one"
    2    => "two"
    _    => "other"
}

-- break / continue
for i in 1..100 {
    if i == 10 { break }
    if i % 2 == 0 { continue }
    show i
}
```

### Functions

```orion
-- Plain function
fn greet(name) {
    return "Hello " + name
}

-- With type hints
fn add(a: int, b: int) -> int {
    return a + b
}

-- Lambda
double = fn(x) { x * 2 }
show double(21)   -- 42

-- Async
async fn fetch(url) {
    resp = net.get(url)
    return resp.body
}
data = await fetch("https://api.example.com")
```

### OOP - shapes

```orion
shape Person {
    name: string = ""
    age:  int    = 0

    on_create(n: string, a: int) {
        name = n
        age  = a
    }

    act greet() {
        show "Hi, I'm " + name
    }

    act birthday() {
        age += 1
    }
}

p = Person("Gabriel", 25)
p.greet()
p.birthday()
show p.age    -- 26

if p is Person { show "It is a Person" }

-- Composition with `using`
shape Animal {
    name: string = ""
    act speak() { show name + " speaks" }
}

shape Dog {
    using Animal
    breed: string = ""
    on_create(n, b) { name = n   breed = b }
    act fetch_ball() { show name + " fetches the ball!" }
}

d = Dog("Rex", "Labrador")
d.speak()
d.fetch_ball()
```

### Error handling

```orion
attempt {
    result = divide(10, 0)
    show result
} handle err {
    show "Error: " + err
}
```

### Native HTTP server

`serve` is a language statement: it takes a port and a handler function.
The handler receives the request and returns a dict with `status` and `body`.

```orion
use "db"

db.ejecutar("app.db", "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT)")

fn router(req) {
    if req["path"] == "/ping" {
        return { "status": 200, "body": "pong" }
    }

    if req["path"] == "/users" {
        if req["method"] == "GET" {
            return { "status": 200, "body": db.query("app.db", "SELECT * FROM users") }
        }
        if req["method"] == "POST" {
            db.insertar("app.db", "INSERT INTO users (name) VALUES (?)", [req["body"]])
            return { "status": 201, "body": { "ok": yes, "message": "Created" } }
        }
    }

    return { "status": 404, "body": "not found" }
}

serve 8080 router
```

**Automatic JSON.** When `body` is a dict or a list, Orion serializes it and
responds with `application/json`. A string body goes out as `text/plain`. An
explicit `content_type` always wins.

```orion
return { "status": 200, "body": {"ok": yes, "total": 3} }
-- → application/json  ·  {"ok":true,"total":3}

return { "status": 200, "body": "pong" }
-- → text/plain  ·  pong
```

For declarative routing with `:id` parameters and wildcards, use the
[`router`](#block-b---modern-web-) module and pass its dispatcher to `serve`.

### Native AI - `think`, `learn`, `sense`

These call an external provider and need an API key. See the `llm` module for
explicit provider and model selection.

```orion
-- No module, no import: AI as a native statement
think "Summarize this text in 3 bullet points: " + content

-- The ai module for higher-level operations
use "ai" as ai

category  = ai.classify(email.text, ["spam", "work", "personal"])
summary   = ai.summarize(document, length: "corto")
translated = ai.translate(text, to: "english")
sentiment = ai.sentiment(review)   -- "positivo" / "negativo" / "neutro"
```

### Pipe operator

```orion
result = data
    |> filter_by("active", yes)
    |> sort_by("date", "desc")
    |> top(10)
```

### Concurrency

```orion
-- Spawn (fire and forget)
spawn long_running_job()

-- Async/await
async fn process(item) { ... }
result = await process(data)
```

---

## Standard library - a tour with examples

### Data and files

```orion
use "fs"
use "csv"
use "json"
use "excel"
use "table"
use "regex" as re
```

#### `fs` - file system
```orion
content = fs.read("config.toml")
fs.write("output.json", data)
files   = fs.ls("data/")
fs.copy("a.txt", "backup/a.txt")
fs.mkdir("reports/2026")
info = fs.info("file.txt")   -- {size, modified, is_file}
```

#### `csv` - tabular data
```orion
data   = csv.read("sales.csv")
north  = csv.filter(data, "region", "North")
stats  = csv.stats(data, "sale")   -- {sum, avg, min, max}
sorted = csv.sort(data, "sale", "desc")
csv.write("report.csv", data)
```

#### `json` - JSON serialization
```orion
obj  = json.parse(text)
txt  = json.forge_pretty(obj)
data = json.absorb("config.json")
json.emit("output.json", data)
val  = json.trace(obj, "user.profile.name")
```

#### `excel` - spreadsheets
```orion
sheets = excel.sheets("report.xlsx")
data   = excel.read("data.xlsx", "Sales")
excel.write("output.xlsx", data, "Report 2026")
```

#### `table` - data analysis
```orion
t = table.load("data.csv")   -- auto-detects CSV / Excel / JSON
table.peek(t, 5)             -- pretty-prints the first 5 rows
table.schema(t)              -- column types
table.profile(t)             -- full statistics

t2 = table.filter(t, "active", yes)
t3 = table.keep(t, ["name", "sale", "region"])
t4 = table.sort(t, "sale")
t5 = table.join(t, t2, "id")
```

#### `regex` - regular expressions
```orion
use "regex" as re

valid = re.is_match("user@example.com", "^[\\w.]+@[\\w]+\\.[\\w]+$")
nums  = re.find_all(text, "\\d+")
clean = re.replace(dirty, "\\s+", " ")
parts = re.groups("2026-05-08", "(\\d{4})-(\\d{2})-(\\d{2})")
words = re.split(line, "[,;]+")
```

### Network and server

```orion
use "net"
use "env"
```

#### `net` - HTTP client
```orion
resp = net.get("https://api.github.com/users/octocat")
data = net.post("https://api.com/data", {token: key, id: 1})
net.download("https://example.com/file.zip", "local/file.zip")
ip   = net.resolve("example.com")
ping = net.pulse("example.com", 443)   -- {alive, latency_ms}
```

#### `env` - configuration
```orion
port = env.pull("PORT", 8080)
mode = env.pull("MODE", "production")
config = env.load(".env")
```

### Utilities

```orion
use "strings"
use "datetime"
use "random"
use "process"
use "log"
```

#### `strings`
```orion
upper  = strings.upper("hi")
parts  = strings.split("a,b,c", ",")
joined = strings.join(list, " - ")
ok     = strings.contains(text, "orion")
b64    = strings.encode_base64(data)
```

#### `datetime`
```orion
now      = datetime.now()
today    = datetime.today()
ts       = datetime.timestamp()
parts    = datetime.parts(now)   -- {year, month, day, hour, ...}
tomorrow = datetime.add_days(today, 1)
diff     = datetime.diff_days("2026-01-01", "2026-12-31")
day      = datetime.weekday(today)   -- "Thursday"
```

#### `random`
```orion
n    = random.int(1, 100)
elem = random.choice(["red", "green", "blue"])
id   = random.uuidv4()
mix  = random.shuffle([1, 2, 3, 4, 5])
```

#### `process`
```orion
res = process.execute("git status")
show res.out
process.background("server.exe")
exists = process.check_dependency("ffmpeg")
```

### Security and cryptography

```orion
use "crypto"
```

```orion
hash  = crypto.sha256("sensitive data")
token = crypto.token(32)
id    = crypto.uuid()

-- Password hashing
h  = crypto.hash(password)
ok = crypto.verify_hash(password, h)

-- HMAC signing
signature = crypto.sign(data, secret)
valid     = crypto.verify(data, signature, secret)

-- Symmetric encryption
encrypted = crypto.encrypt(data, key)
plain     = crypto.decrypt(encrypted.cipher, encrypted.key)
```

### AI and vision

`ai` and `insight` call an external provider and need an API key.
`vision.ocr` runs locally with embedded models.

```orion
use "ai"
use "vision"
use "insight"
```

```orion
-- ai
summary    = ai.summarize(text)
category   = ai.classify(email, ["spam", "work", "personal"])
code       = ai.code("function that sorts a list of dicts by date")
sentiment  = ai.sentiment(review)
translated = ai.translate(text, to: "english")
extracted  = ai.extract(invoice, ["number", "date", "total"])

-- vision
info = vision.info("photo.jpg")       -- {width, height}
vision.resize("photo.jpg", 800, 600, "thumb.jpg")
vision.grayscale("photo.jpg", "gray.jpg")
b64  = vision.to_base64("photo.jpg")

-- insight (AI over documents)
analysis = insight.analyze("contract.png", "What is the expiry date?")
```

### Scientific and simulation

```orion
use "matrix"
use "quantum"
use "cosmos"
```

```orion
-- matrix - numerical linear algebra (nalgebra engine from 32×32 up:
-- BLAS-style multiply, LU with pivoting; 512×512 in tens of ms)
A   = [[1,2],[3,4]]
det = matrix.det(A)
inv = matrix.inverse(A)
x   = matrix.solve([[1,1],[1,-1]], [3, 1])   -- linear systems via LU
e   = matrix.eig([[2,1],[1,2]])              -- eigenvalues: [1.0, 3.0]
s   = matrix.svd(A)                          -- {u, s, vt}
r   = matrix.rank([[1,2],[2,4]])             -- 1 (numerical rank)

-- quantum - a real CIRCUIT simulator (up to 24 qubits, O(2^n) gates
-- parallelized; phase matters, so Grover works in plain Orion)
c = quantum.circuit(2)
quantum.h(c, 0)                     -- Hadamard on qubit 0
quantum.cnot(c, 0, 1)               -- a Bell pair you build yourself
quantum.rx(c, 0, 3.14159)           -- parametric rotations (rx/ry/rz/phase)
quantum.ugate(c, 0, [[0,1],[1,0]])  -- your own 2×2 gate (unitarity checked)
show quantum.probs(c)               -- {"00": 0.5, "11": 0.5}
m = quantum.sample(c, 1000)         -- Born rule, no collapse
b = quantum.collapse(c, 0)          -- measures one qubit and COLLAPSES the state
-- Full Grover in demo/demo_grover.orx (P=0.945 exactly) and an animated
-- Bloch sphere with real physics in demo/demo_bloch_anim.orx

-- cosmos - N-body simulation
u = cosmos.create(5)
u = cosmos.run(u, steps: 100)
show cosmos.summary(u)
```

---

## The CLI

```bash
# Run
orion file.orx

# Interactive REPL
orion

# New project scaffold
orion new my-api

# Check syntax
orion check main.orx

# Check static types
orion check main.orx --types

# Hot reload on save
orion watch main.orx

# Benchmark
orion bench main.orx --runs=20

# Auto-discovered tests (test_*.orx)
orion test
orion test tests/

# Environment diagnostics
orion doctor
```

### REPL

```
orion> 2 + 3
5
orion> name = "Orion"
orion> "Hello " + name
"Hello Orion"
orion> fn double(x) { return x * 2 }
orion> double(21)
42
orion> :vars     ← show live variables
orion> :fns      ← show defined functions
orion> :clear    ← reset the state
orion> :exit     ← quit
```

### What `orion new` generates

```
my-api/
├── main.orx          ← a working backend server
├── orion.json        ← project manifest
├── .env.example
├── .gitignore
├── lib/
│   └── utils.orx
└── test/
    └── test_routes.orx
```

---

## Architecture

Orion is **not a tree-walking interpreter** - that legacy was removed. It is a
bytecode compiler with **three execution backends** that share one frontend and
produce identical results, verified by differential tests. Around 13,600 lines
of core Rust plus 58 native modules.

```
file.orx
    │
    ▼
lexer.rs        ← tokenization (UTF-8, ${} interpolation, escapes)
    │
    ▼
parser.rs       ← recursive descent AST
    │
    ▼
typechecker.rs  ← type checking (on by default; opt out with --no-typecheck)
    │
    ▼
codegen.rs      ← AST → bytecode
    │
    ▼
  bytecode
    │
    ├──►  vm.rs               ← bytecode VM (default). Native Rust, no GIL.
    │
    ├──►  jit/  (--jit)       ← JIT to machine code via Cranelift.
    │                            Falls back to the VM automatically when an
    │                            instruction is not yet supported in the JIT.
    │
    └──►  aot.rs  (--build)   ← AOT compilation to a standalone native binary.
```

Runtime subsystems shared by all three backends:

- **Mark-and-sweep GC** ([`gc.rs`](orion-vm/src/gc.rs)) - collects reference
  cycles; both *mark* and *drop* are iterative, so nesting depth is unbounded.
- **Checked arithmetic** - integer overflow is an explicit error, never a silent wrap.
- **Concurrency** - `spawn`/`await` on a cached thread pool
  ([`task_pool.rs`](orion-vm/src/task_pool.rs)), `chan` channels and thread-safe
  shared state (the `state` module).
- **DAP debugger** ([`dap.rs`](orion-vm/src/dap.rs)) - real breakpoints, stepping
  and watches from VS Code.

**No Python. No external runtime. A single executable.**

---

## Performance - measured, not promised

Reproducible benchmark in [`bench/`](bench/), one command: `bench\run_all.ps1`.
Same task in both languages: load 500k CSV rows into typed columns, then `sum`
and `mean`. The numeric results match **digit for digit**, so the benchmark
doubles as a cross-language correctness test.

| Pipeline (500k rows × 4 cols)     | Time    | Peak RAM |
|----------------------------------|---------|----------|
| Python 3.13 (csv stdlib, in C)   | 516 ms  | 105 MB   |
| **Orion `frame.open` CSV**       | **264 ms** | **104 MB** |
| **Orion `frame.open` .odf**      | **88 ms**  | **73 MB**  |

- **CSV: 2× faster than Python at the same memory** - columnar loading in Rust;
  cells go straight into a per-column `Vec`, and text columns are moved without
  reallocating.
- **.odf (Orion's own binary format): about 6× faster** - no text parsing at all,
  numbers are read as raw bytes.
- **At 5M rows**: 46% less peak RAM on load, plus data-parallel aggregations via
  rayon (`sum/std/min/max` use every core from 1M elements up).
- **And in 3× fewer lines**: Python's ~15 lines of manual loop and typing become
  5 lines of Orion, since `frame.open` infers types and layout on its own.

Beyond throughput, the runtime is hardened for large data: structures nested
200k+ levels deep and reference cycles (`push(a, a)`) neither crash nor leak.
The GC collects them and the VM returns every byte on exit, verified with
LeakSanitizer in CI.

---

## VS Code extension

![VS Code Extension](assets/demo_vscode.jpeg)

- Full syntax highlighting
- IntelliSense through an integrated LSP
- Real compiler diagnostics as you type
- Code lenses: `▶ Run` plus complexity metrics
- Watch mode with output in a panel
- Visual shape diagram
- Route explorer with a built-in REST client
- Test explorer that discovers `test_*.orx`
- Import graph
- DAP debugger
- Integrated REPL
- **On-demand compiler** - if `orion` is not on your `PATH`, the extension
  downloads it from the latest release and keeps it up to date. Still
  zero-config, without inflating the `.vsix`.

---

## Runtime status

| Component | Status | Technology |
|---|---|---|
| Lexer + escape sequences | ✅ Complete | Rust |
| Parser | ✅ Complete | Rust |
| Type checker | ✅ Complete | Rust |
| Bytecode compiler | ✅ Complete | Rust |
| VM (execution) | ✅ Complete | Rust |
| OOP (shape, act, using, is) | ✅ Complete | Rust |
| Optional type hints | ✅ Complete | Rust |
| Error handling (attempt/handle) | ✅ Complete | Rust |
| Async / await | ✅ Complete | Rust |
| Interactive REPL | ✅ Complete | Rust |
| Native HTTP server | ✅ Complete | Rust |
| Native AI (think/learn/sense) | ✅ Complete | Rust |
| Errors with spans and visual context | ✅ Complete | Rust |
| Interactive debugger (breakpoints, step, watches) | ✅ Complete | Rust |
| DAP - Debug Adapter Protocol (VS Code) | ✅ Complete | Rust |
| LSP - real-time diagnostics | ✅ Complete | Rust |
| JIT - Cranelift (I/O, modules, OOP) | ✅ Complete | Cranelift |
| AOT - standalone native executable (needs a C toolchain: MSVC Build Tools, or MinGW/gcc on the PATH) | ✅ Complete | Cranelift |
| FFI - external native libraries | ✅ Complete | libloading |
| Package manager (add/remove/list/search/publish) | ✅ Complete | Rust |
| Official registry on GitHub | ✅ Complete | GitHub API |
| Mark-and-sweep GC (cycles; iterative mark and drop, unbounded depth) | ✅ Complete | Rust |
| Zero leaks on exit (verified with LeakSanitizer in CI) | ✅ Complete | Rust + ASan |
| Reproducible benchmark vs Python ([`bench/`](bench/)) | ✅ Complete | PowerShell + Python |
| Standard library modules | ✅ 58 modules (875 functions) | Rust |
| Cloud native (S3 / SSH / Docker) | ✅ Complete | Rust |
| Full CLI | ✅ Complete | Rust |
| VS Code extension (published on the Marketplace) | ✅ Complete | TypeScript |

---

## Full standard library (58 modules)

### Core
`fs` `json` `strings` `datetime` `random` `regex` `env` `process` `crypto` `term`

### System
`log` `config` `secret` `zip` `stream` `crypto2` `state`

### Network and web
`net` `ws` `serve` `router` `middleware` `sse` `proto`

### Backend
`db` `auth` `cache` `mail` `validate`

### Automation
`tarea` `cola` `watch`

### Data and science
`csv` `excel` `excel_f` `table` `frame` `serie` `stat` `matrix` `search`

### Utilities
`template` `formato` `grafo` `pdf`

### Native AI (block C)
`llm` `embed` `vector` `ai`

### Interfaces
`gui` `tui`

### Advanced
`vision` `insight` `quantum` `cosmos` `timewarp`

### Cloud native (block E)
`s3` `ssh` `docker`

---

## Ecosystem

> Orion does not copy Python. Each module is designed for a simple, fast API
> that needs no configuration.

### Where Orion differs from Python

| | Python | Orion |
|---|---|---|
| Speed | slower (GIL) | native Rust + JIT |
| Startup | 150-400 ms | < 1 ms |
| Built-in AI | pip install | standard library |
| Native compilation | no | `orion --build` |
| Package manager | pip | `orion --add` |
| API design | 1990s legacy | designed from scratch |

---

### Block D - System ✅
*The base of any real application.*

| # | Module | Description | Rust crate | Status |
|---|--------|-------------|------------|--------|
| 1 | `use "zip"` | Compress and extract gzip, zip, tar | `flate2` + `zip` | ✅ Complete |
| 2 | `use "secret"` | Read `.env`, safe secrets with validation | native | ✅ Complete |
| 3 | `use "log"` | Structured logging with levels, colors, timers and files | native | ✅ Complete |
| 4 | `use "config"` | Load TOML / JSON as typed configuration | `toml` | ✅ Complete |
| 5 | `use "crypto2"` | AES-256-GCM, RSA, signing and verification | `aes-gcm` + `rsa` | ✅ Complete |
| 6 | `use "stream"` | Data pipelines: filter, pluck, sum, avg, unique, flatten | native | ✅ Complete |

```orion
-- log - structured logging with tags, timers and dividers
use "log"

log.divider("start")
log.info("Server starting on port 8080", "startup")
log.timer("db")
log.info("Connecting to the database...", "DB")
log.ok("Connection established", "DB")
log.elapsed("db", "connection")     -- OK  [db]  connection completed in 12ms
log.warn("Token expiring soon", "auth")
log.err("User not found", "auth")
log.level("debug")                  -- enable debug messages
log.debug("Request: GET /api/v1/users", "net")
log.divider()

-- config - load TOML / JSON as typed configuration
use "config"

cfg  = config.load("orion.toml")
port = config.get(cfg, "server.port")
cfg2 = config.merge(cfg, "local.toml")   -- local.toml overrides

-- secret - safe secrets from .env
use "secret"

secret.load(".env")
db_url  = secret.require("DATABASE_URL")   -- clear error if missing
api_key = secret.get("API_KEY", "dev")
show secret.mask(api_key)                  -- "sk***y"

-- zip - compress and extract
use "zip"

zip.compress("src/", "release.zip")      -- compresses a whole folder
n = zip.decompress("release.zip", "out/")
entries = zip.list("release.zip")        -- [{name, size, is_dir}, ...]
zip.gzip("data.csv", "data.csv.gz")
zip.gunzip("data.csv.gz", "data.csv")

-- stream - data pipelines with no dependencies
use "stream" as st

users = [
    {"name": "Ana",  "active": yes, "sale": 4200},
    {"name": "Luis", "active": no,  "sale": 1800},
    {"name": "Eva",  "active": yes, "sale": 3100}
]

active = st.where_(users, "active", yes)
names  = st.pluck(active, "name")             -- ["Ana", "Eva"]
total  = st.sum(st.pluck(active, "sale"))     -- 7300
top3   = st.take(st.reverse(st.range(1, 100)), 3)  -- [99, 98, 97]

-- crypto2 - AES-256-GCM and RSA
use "crypto2"

-- AES-256-GCM (authenticated symmetric encryption)
encrypted = crypto2.aes_encrypt("sensitive data", "my-secret-key")
plain     = crypto2.aes_decrypt(encrypted, "my-secret-key")

-- RSA (asymmetric encryption + digital signature)
keys      = crypto2.rsa_keygen()            -- {public_key, private_key}
c         = crypto2.rsa_encrypt("message", keys.public_key)
m         = crypto2.rsa_decrypt(c, keys.private_key)
signature = crypto2.rsa_sign("contract", keys.private_key)
valid     = crypto2.rsa_verify("contract", signature, keys.public_key)  -- yes
```

---

### Block B - Modern web ✅
*Beyond the basic `serve`: middleware, advanced routing, modern protocols.*

| # | Module | Description | Rust crate | Status |
|---|--------|-------------|------------|--------|
| 7 | `use "router"` | Declarative routing with `:id` parameters and `*` wildcards | native | ✅ Complete |
| 8 | `use "middleware"` | Rate limiting, CORS, logging, JWT auth in a chain | native | ✅ Complete |
| 9 | `use "sse"` | Server-Sent Events for real-time HTTP streaming | native | ✅ Complete |
| 10 | `use "proto"` | MessagePack binary serialization, more compact than JSON | native | ✅ Complete |

```orion
-- router + serve together - the full combination
use "router"
use "middleware"

limiter = middleware.rate_limit(100, 60)   -- 100 req / 60 s

-- Handlers are NAMED functions: you pass the function NAME to the
-- router as a string, not a lambda. serve runs each request in its
-- own VM and looks handlers up by name, so an anonymous lambda
-- cannot be dispatched.
fn mw_global(req) {
    if not middleware.check_rate(limiter, req["path"]) {
        return {"status": 429, "body": "Too Many Requests"}
    }
    return null   -- null = continue to the handler
}

fn view_user(req) {
    return {"status": 200, "body": "User: " + req["params"]["id"]}
}

fn create_user(req) {
    return {"status": 201, "body": req["body"]}
}

fn view_file(req) {
    return {"status": 200, "body": "File: " + req["params"]["rest"]}
}

fn fallback(req) {
    return {"status": 404, "body": "not found"}
}

r = router.new()
router.use_middleware(r, "mw_global")
router.get(r,  "/users/:id",     "view_user")
router.post(r, "/users",         "create_user")
router.get(r,  "/files/*rest",   "view_file")
router.attach(r)   -- activates the router for the next serve

-- The router dispatches automatically; `fallback` handles anything that
-- does not match. `serve` always takes a port plus a handler function.
serve 8080 fallback

-- router.match() can also be used manually
match = router.match(r, "GET", "/users/42")
-- {method: GET, path: /users/42, params: {id: 42}, handler: view_user}

show router.routes(r)   -- lists every registered route

-- middleware - rate limiting, CORS, JWT auth
use "middleware"

limiter = middleware.rate_limit(100, 60)   -- 100 req / 60 s
ok = middleware.check_rate(limiter, "192.168.1.1")   -- yes / no

cors_headers = middleware.cors("https://myapp.com", "GET, POST", "Authorization")
result = middleware.auth_bearer(token, "my-secret")
-- {valid: yes, sub: "user123", payload: {rol: "admin", exp: 1800000000}}

middleware.log_req("GET", "/api/users", 200, 12)
-- 14:32:01  GET     /api/users   200  12ms

-- sse - Server-Sent Events
use "sse"

headers = sse.headers()   -- {Content-Type: "text/event-stream", ...}
ev = sse.event("test message")              -- "data: test message\n\n"
ev = sse.named("update", "new data")        -- "event: update\ndata: new data\n\n"
ev = sse.json_event("users", [{name: "Ana"}])
ev = sse.retry(3000)                        -- "retry: 3000\n\n"
ev = sse.keep_alive()                       -- ": keep-alive\n\n"

-- proto - MessagePack binary serialization
use "proto"

data  = {name: "Ana", age: 25, active: yes}
bytes = proto.encode(data)        -- list of ints (bytes)
b64   = proto.encode_b64(data)    -- base64 string
show proto.size(data)             -- size in bytes (smaller than JSON)
show proto.json_size(data)        -- size as JSON, for comparison

restored = proto.decode(bytes)
restored = proto.decode_b64(b64)
```

---

### Block C - Native AI ✅
*First-class AI, without pip and without configuration. These modules call
external providers and need an API key.*

| # | Module | Description | Rust crate | Status |
|---|--------|-------------|------------|--------|
| 11 | `use "llm"` | One-line calls to OpenAI / Anthropic / Ollama / Gemini | `ureq` | ✅ Complete |
| 12 | `use "embed"` | Text embeddings, cosine similarity, semantic search | native math | ✅ Complete |
| 13 | `use "vector"` | In-memory vector database with cosine similarity | native | ✅ Complete |

> **Separation of concerns:**
> - `ai.*` → high level, no model choice (summarize, classify, sentiment, translate)
> - `llm.*` → direct model control (query with an explicit provider, multi-turn chat)
> - `embed.*` → vectors only (text → embedding, similarity, semantic search)

```orion
use "llm"
use "embed"    -- alias de "embeddings"
use "vector"

-- Multi-provider: claude, gpt, gemini, ollama
answer = llm.query("gpt-4o", "Summarize this contract in 3 points: " + contract)
answer = llm.query("claude-sonnet-4-6", prompt)
answer = llm.query("ollama:llama3", prompt)
answer = llm.query("gemini-2.0-flash", prompt)
answer = llm.query("auto", prompt)   -- detects the configured provider

-- With a system prompt
r = llm.query_with("gpt-4o", question, "You are a legal expert.")

-- Multi-turn chat
msgs = [
    {"role": "user",      "content": "Hi"},
    {"role": "assistant", "content": "Hello!"},
    {"role": "user",      "content": "What is 2+2?"}
]
r = llm.chat("claude-haiku-4-5-20251001", msgs)

-- Embeddings
vec = llm.embed("text-embedding-3-small", text)   -- List<float>

-- Semantic search over a small corpus (no vector DB)
results = embed.search("When was it founded?", documents, top: 3)
-- → [{text: "...", score: 0.91, index: 4}, ...]

-- Cosine similarity between two vectors
sim  = embed.similarity(emb1, emb2)   -- 0.0 .. 1.0
dist = embed.distance(emb1, emb2)
norm = embed.normalize(emb1)

-- In-memory vector database
db = vector.new()
for doc in corpus {
    v = embed.text(doc.text)
    vector.add(db, doc.id, v, doc.title)
}
query_vec = embed.text("When was the company founded?")
results   = vector.buscar(db, query_vec, 5)
-- → [{id: "doc-12", score: 0.934, metadata: "History"}, ...]
vector.save(db, "corpus.vdb.json")   -- persist to JSON
db2 = vector.load("corpus.vdb.json") -- load back

-- Available providers
show llm.providers()   -- ["anthropic", "openai", "gemini", "ollama"]
show llm.models()      -- ["claude-haiku-4-5-20251001", "gpt-4o", "ollama:llama3:latest", ...]
```

---

### Block A - Modern data
*A pandas replacement: faster, simpler API, no heavy dependencies.*

| # | Module | Description | Implementation | Status |
|---|--------|-------------|----------------|--------|
| 14 | `use "table"` / `use "df"` | Row-oriented dataframes: load, filter, group, join, forecast | native Vec | ✅ Complete |
| 15 | `use "frame"` | **Columnar** dataframes: far less RAM, chunk streaming, scan without loading | columnar Vec | ✅ Complete |
| 16 | `use "stat"` | Statistics: mean, std, percentile, correlation, regression, z-score, histogram | native Vec | ✅ Complete |
| 17 | `use "serie"` | Time series: moving_avg, diff, pct_change, forecast, trend, smooth | native Vec | ✅ Complete |
| 18 | `use "search"` | Fast search across TXT/CSV/Excel/dirs - streaming, regex, context, multi-column | native BufReader | ✅ Complete |

> No polars, no ndarray, no heavy dependencies. The split: `table` for quick
> exploration, `frame` for production and large volumes.

#### Which one to use

| Volume | Module | Why |
|---------|--------|---------|
| < 50K rows | `table` | Richer API, exploration, built-in AI |
| 50K - 5M rows | `frame` | Columnar, far less RAM, operations straight on `Vec<f64>` |
| > 5M rows | `frame.each_chunk` / `frame.scan_stats` | Never loads everything, processes in blocks |
| Searching files | `search` | Streaming, stops at the first match, multi-file |

```orion
use "table"     -- or: use "df"

-- Load: auto-detects CSV / Excel / JSON
t = table.load("sales.csv")
table.peek(t, 5)       -- prints the first 5 rows
table.schema(t)        -- column types
table.profile(t)       -- full statistics

-- Filter, select, sort
north = table.where(t, "region == 'North' && active == yes")
top10 = table.top(t, "sale", 10)
t2    = table.keep(t, ["name", "region", "sale"])
t3    = table.sort(t, "sale", "desc")

-- Computed column
t4 = table.add(t, "total", "sale * 1.19")

-- Aggregation
by_region = table.group(t, "region", "sale", "sum")
stats     = table.stats(t, "sale")   -- {min, max, avg, std, p25, median, p75}

-- Combine
joined = table.join(t, t2, "id")
all    = table.concat(t, t2)

-- Analytics
pred     = table.forecast(t, "sale", 5)      -- linear projection
outliers = table.anomalies(t, "sale")        -- IQR outliers
corr     = table.correlate(t, "age", "sale") -- Pearson
ranked   = table.rank(t, "sale")             -- adds _rank and _pct
mavg     = table.moving_avg(t, "sale", 3)    -- moving average

-- Save: format auto-detected from the extension
table.save(t, "report.csv")
table.save(t, "report.xlsx")
table.save(t, "report.json")

-- AI integration (calls an external provider)
table.describe_ai(t)       -- AI-generated description
resp = table.ask(t, "Which region sells most in summer?")
```

#### `frame` - columnar dataframes for large volumes

```orion
use "frame"

-- Direct columnar load, without materializing rows: 2× faster than the
-- Python standard library at the same memory - measured in bench/
-- (500k and 5M rows). open() auto-detects the format: CSV, or the .odf
-- binary format, which is about 6× faster.
f = frame.open("sales_1M.csv")
frame.schema(f)          -- inferred column types
frame.peek(f, 5)         -- pretty table without loading everything
frame.size(f)            -- {rows: 1000000, cols: 8}

-- Stats straight on Vec<f64> - no hash lookups; from 1M elements up they
-- use every core (rayon)
frame.mean(f, "sale")
frame.stats(f, "sale")   -- {count, mean, std, min, p25, median, p75, max}

-- Filter, select, sort
north  = frame.where_(f, "region", "North")
top    = frame.sort(f, "sale", "desc")
simple = frame.keep(f, ["name", "region", "sale"])

-- Columnar aggregation
by_region = frame.group(f, "region", "sale", "sum")

-- Large files: process in 10K chunks without loading everything
chunks = frame.each_chunk("sales_100M.csv", 10000)
for chunk in chunks {
    stats = frame.stats(chunk, "sale")
    show "Chunk mean: ${stats.mean}"
}

-- Full scan of one column without loading the file
stats = frame.scan_stats("sales_100M.csv", "sale")
-- → {count, mean, std, min, max, sum} - iterates only that column
```

#### `search` - fast search in any file

```orion
use "search"

-- TXT / LOG - streaming, never loads everything into RAM
errors = search.text("app.log", "ERROR")
-- → [{line: 42, content: "ERROR: connection refused"}, ...]

-- Regex with captured groups
dates = search.regex("file.txt", "(\\d{4}-\\d{2}-\\d{2})")
-- → [{line, content, matches: ["2026-05-15"]}, ...]

-- CSV - search by column without loading the file
customers = search.csv("customers.csv", "city", "Monterrey")
-- → [{name: "Ana", city: "Monterrey", ...}, ...]

-- CSV - search across several columns
hits = search.columns("products.csv", ["name", "description"], "orion")

-- Excel - search a whole sheet
rows = search.excel("report.xlsx", "pending")
rows = search.excel("report.xlsx", "North", "Q1 Sales")  -- specific sheet

-- Type auto-detected from the extension
result = search.in_file("data.csv", "Ana")       -- CSV
result = search.in_file("notes.txt", "urgent")   -- text
result = search.in_file("base.xlsx", "error")    -- Excel

-- Count without materializing (very fast on large files)
n = search.count("logs/app.log", "CRITICAL")

-- First match, then stop (ideal for verification)
first = search.first("customers.csv", "Ana García")

-- Search every file in a directory
hits = search.in_dir("logs/", "timeout")        -- all files
hits = search.in_dir("data/", "North", "csv")   -- only .csv

-- Context - N lines before and after (like grep -C)
ctx = search.context("deploy.log", "FAILED", 3)
-- → [{line, content, before: [...], after: [...]}]
```

---

### Block E - Cloud native ✅
*No pip, no npm. Cloud as part of the standard library.*

| # | Module | Description | Rust crate | Status |
|---|--------|-------------|------------|--------|
| 18 | `use "s3"` | Upload and download files to S3 / R2 / MinIO | `ureq` + AWS Sig V4 | ✅ Complete |
| 19 | `use "ssh"` | Run remote commands over SSH, plus SCP | `ssh2` | ✅ Complete |
| 20 | `use "docker"` | Control Docker containers through the REST API | `ureq` | ✅ Complete |

```orion
-- s3 - works with AWS S3, Cloudflare R2 and MinIO
use "s3"

s3.config("https://s3.amazonaws.com", env.pull("AWS_KEY"), env.pull("AWS_SECRET"), "us-east-1")

-- Upload a file
r = s3.upload("my-bucket", "backups/report.csv", "report.csv")
show r.url   -- https://s3.amazonaws.com/my-bucket/backups/report.csv

-- Download a file
s3.download("my-bucket", "backups/report.csv", "local/report.csv")

-- List objects
files = s3.list("my-bucket", "backups/")
for f in files { show f.key + "  " + f.size }

-- Check existence and delete
if s3.exists("my-bucket", "backups/old.csv") {
    s3.delete("my-bucket", "backups/old.csv")
}

-- MinIO / R2 - same API, different endpoint
s3.config("http://localhost:9000", "minio", "minio123", "us-east-1")
s3.upload("data", "file.json", "output.json")

-- Cloudflare R2
s3.config("https://<account>.r2.cloudflarestorage.com", env.pull("R2_KEY"), env.pull("R2_SECRET"), "auto")


-- ssh - remote connection with a password or a key
use "ssh"

-- Password
s = ssh.connect("192.168.1.10", 22, "deploy", "secret")

-- Private key
s = ssh.connect_key("server.com", 22, "ubuntu", "/home/user/.ssh/id_rsa")

-- Run commands
r = ssh.exec(s, "df -h")
show r.out    -- disk usage
show r.code   -- 0 = success

r = ssh.exec(s, "systemctl status nginx")
show r.out

-- Upload and download files (SCP)
ssh.upload(s, "dist/app.tar.gz", "/opt/app/app.tar.gz")
ssh.download(s, "/var/log/app.log", "logs/app.log")

-- Check the connection
if ssh.test(s) { show "server reachable" }

ssh.close(s)


-- docker - control the daemon through the REST API
use "docker"

-- Configure the endpoint (default: http://localhost:2375)
docker.config("http://localhost:2375")

-- Check the daemon
if docker.ping() { show "Docker is up" }
show docker.version()   -- {version, api_version, os, arch}

-- Containers
cs = docker.containers()           -- running only
cs = docker.containers(yes)        -- all, including stopped
for c in cs { show c.name + "  " + c.status }

-- Lifecycle
docker.start("my-api")
docker.stop("my-api", 10)    -- 10s grace period
docker.restart("my-api")
docker.kill("my-api")
docker.remove("my-api", yes)  -- force=yes

-- Logs
show docker.logs("my-api", 50)    -- last 50 lines

-- Inspect
info = docker.inspect("my-api")
show info.State.Status

-- Launch a new container
c = docker.run("nginx:latest", {
    name: "web",
    env:  ["PORT=8080", "ENV=prod"],
    cmd:  ["nginx", "-g", "daemon off;"]
})
show "Started: " + c.id

-- Images
imgs = docker.images()
for i in imgs { show i.tags }
docker.pull("redis:7")

-- Live metrics
st = docker.stats("my-api")
show "CPU: " + st.cpu_pct + "%"
show "RAM: " + st.mem_usage + " / " + st.mem_limit
```

---

### Implementation order

```
Block D ✅ → Block B ✅ → Block C ✅ → Block A ✅ → Block E ✅
 (base)       (web)        (AI)        (table/df)    (cloud)
```

---

## Roadmap - Excel and automation

> Orion does not copy pandas or openpyxl. Each feature has its own name, a
> cleaner API, and works with `|>`.

### Current state of the `excel` module

```orion
use "excel" as excel

-- What already works today
data  = excel.read("sales.xlsx")
data  = excel.filter(data, "active", "==", yes)
data  = excel.group(data, "region", { "sales": "sum", "count": yes })
data  = excel.sort(data, "region")          -- single column
data  = excel.join(data, targets, "region") -- single key
stats = excel.stats(data, "sales")
excel.write_styled("report.xlsx", data, { ... })

-- Data plus a chart in one file, in a single call
excel.write_styled("report.xlsx", data, {
    titulo:  "Q1 Sales Report",
    stripe:  yes,
    freeze:  yes,
    charts: [
        {
            type:        "bars",
            x:           "region",
            y:           "sales_sum",
            title:       "Sales by Region",
            palette:     "orion",
            style:       "minimal",
            show_values: yes,
            sheet:       "Chart"
        }
    ]
})
```

### The nine designed features

| # | Feature | Pandas equivalent | Status |
|---|---|---|---|
| 1 | `compute` | `df["col"].apply(fn)` | ✅ Complete |
| 2 | `sort`, multi-column | `sort_values(["a","b"])` | ✅ Complete |
| 3 | `group`, multi-agg | `groupby().agg({...})` | ✅ Complete |
| 4 | `long` | `df.melt(...)` | ✅ Complete |
| 5 | `dates` + `date_parts` | `pd.to_datetime(...)` | ✅ Complete |
| 6 | `join`, multi-key | `merge(on=["a","b"])` | ✅ Complete |
| 7 | `chart` | openpyxl charts | ✅ Complete |
| 8 | `formula` | `ws["A1"] = "=SUM(...)"` | ✅ Complete |
| 9 | `sheet` builder | openpyxl cell-level | Next |

---

### F-1 `compute` - computed columns

The lambda receives the whole row, so fields can reference each other. Several
columns in a single pass.

```orion
data = excel.compute(data, {
    "bonus":    fn row => row["sales"] * 0.05,
    "tier":     fn row => if row["sales"] > 90000 { "A" } or if row["sales"] > 70000 { "B" } else { "C" },
    "on_track": fn row => row["sales"] >= row["target"]
})
```

---

### F-2 `sort` - multiple columns

```orion
-- Explicit style
data = excel.sort(data, [
    { by: "region", dir: "asc" },
    { by: "sales",  dir: "desc" }
])

-- Short Orion style: + is ascending, - is descending
data = excel.sort(data, "region+", "sales-", "name+")
```

---

### F-3 `group` - several aggregations per field

```orion
by_region = excel.group(data, "region", {
    "sales":  ["sum", "avg", "max", "min"],
    "months": ["avg"],
    "count":  yes
})
-- Produces: sales_sum, sales_avg, sales_max, sales_min, months_avg, count
```

Available functions: `sum` `avg` `max` `min` `count` `first` `last` `std` `median`

---

### F-4 `long` - wide to long (unpivot)

Turns wide format into long format. A clear name: `long`, not `melt`.

```orion
-- Before (wide): region | CRM Pro | Analytics | Cloud
-- After (long):  region | product | sales

long_data = excel.long(wide_data,
    keep: ["region", "seller"],
    var:  "product",
    val:  "sales"
)
```

---

### F-5 `dates` and `date_parts`

Integrated with the `datetime` module. Works in a `|>` pipeline.

```orion
data = data
    |> excel.dates("sale_date", "DD/MM/YYYY")
    |> excel.date_parts("sale_date", ["year", "month", "quarter", "weekday"])
    |> excel.group("quarter", { "sales": ["sum", "avg"] })
```

Formats: `"DD/MM/YYYY"` `"MM/DD/YYYY"` `"YYYY-MM-DD"` `"auto"`

Parts: `"year"` `"month"` `"day"` `"quarter"` `"weekday"` `"week"` `"hour"`

---

### F-6 `join` - multiple keys

```orion
-- Single key (unchanged)
data = excel.join(sellers, targets, "region", "left")

-- Multiple keys
data = excel.join(sellers, targets, ["region", "product"], "left")
```

---

### F-7 `chart` - declarative charts in Excel

No intermediate objects, no manual series. One call.

```orion
excel.chart("report.xlsx", by_region, {
    type:  "bars",
    x:     "region",
    y:     "sales_sum",
    title: "Sales by Region Q1",
    sheet: "Charts"
})

-- Multiple series
excel.chart("report.xlsx", by_month, {
    type:  "lines",
    x:     "month",
    y:     ["sales_sum", "target_sum"],
    title: "Sales vs Target"
})
```

Types: `"bars"` `"stacked_bars"` `"lines"` `"area"` `"pie"` `"scatter"`

---

### F-8 `formula` - live formulas in Excel

Orion does not expose raw Excel formula strings. Instead there is a builder with
clear names. Columns marked as formulas stay live in the file and recalculate
when opened in Excel.

```orion
f = excel.f

excel.write_styled("report.xlsx", data, {
    formulas: {
        "bonus":   f.pct("sales", 5),
        "total":   f.sum("sales"),
        "rank":    f.rank("sales", "desc"),
        "ratio":   f.ratio("sales", "target")
    }
})
```

Functions: `f.sum` `f.avg` `f.pct` `f.ratio` `f.rank` `f.cumulative` `f.if_`

---

### F-9 `sheet` - full cell-by-cell control

A declarative builder. No manual cell iteration.

```orion
sheet = excel.sheet("Sales Report")

sheet.put("A1", "Q1 2026 - Sales Report", { bold: yes, size: 16, merge: "A1:F1" })
sheet.put("A2", "Generated: " + datetime.today(), { color: "#888888" })
sheet.data("A4", sellers, { header: yes, stripe: yes })
sheet.chart("H4", { type: "bars", x: "region", y: "sales", width: 400, height: 300 })
sheet.style("A4:F4", { bg: "#1B4F72", color: "#FFFFFF", bold: yes })
sheet.freeze("A5")
sheet.autofilter("A4:F4")

excel.save(sheet, "custom_report.xlsx")
```

---

### The full pipeline, in a single API

```orion
use "excel" as excel

excel.read("sales_q1.xlsx")
    |> excel.filter("active", "==", yes)
    |> excel.dates("sale_date", "DD/MM/YYYY")
    |> excel.date_parts("sale_date", ["month", "quarter"])
    |> excel.compute({
        "bonus": fn row => row["sales"] * 0.05,
        "tier":  fn row => if row["sales"] > 90000 { "A" } else { "B" }
    })
    |> excel.group("quarter", { "sales": ["sum", "avg"], "count": yes })
    |> excel.sort("quarter+")
    |> excel.write_styled("q1_report.xlsx", {
        title:      "Q1 Sales Analysis",
        stripe:     yes,
        freeze:     yes,
        autofilter: yes
    })

excel.chart("q1_report.xlsx", by_quarter, {
    type:  "bars",
    x:     "quarter",
    y:     "sales_sum",
    title: "Sales by Quarter"
})
```

### Implementation order

| # | Feature | Impact | Estimated time |
|---|---|---|---|
| 1 | `compute` | Very high | 2-3h |
| 2 | `sort`, multi-column | High | 1-2h |
| 3 | `group`, multi-agg | High | 3-4h |
| 4 | `join`, multi-key | Medium | 1-2h |
| 5 | `dates` + `date_parts` | High | 3-4h |
| 6 | `long` | Medium | 2-3h |
| 7 | `chart` | Very high | 4-6h |
| 8 | `formula` | Medium | 3-4h |
| 9 | `sheet` builder | High | 6-8h |

---

## Contributing

```bash
# Add a module to the standard library
# 1. Create orion-vm/src/modules/my_module.rs
# 2. Register it in orion-vm/src/modules/mod.rs
# 3. Add the dependency to orion-vm/Cargo.toml

# Publish an .orx package to the official registry
orion --publish   # requires orion.json + ORION_GITHUB_TOKEN
```

When you add a function to a module, `scripts/gen_builtins.js` picks it up from
the `match` arm and its `// name(args) → description` comment, and regenerates
the builtins registry on every build. That registry feeds `orion --builtins-json`,
the editor autocompletion and the type checker, so a function missing from it is
reported as non-existent. The `registry_matches_runtime` test guards both
directions.

---

*Orion - built by Angel Zapata · 2025-2026*
