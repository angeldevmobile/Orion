# The Orion standard library

61 modules compiled into the `orion` executable: nothing to install, no
package manager needed for any of them. Import one with `use "name"` (or
`use "name" as alias`). How far each module is verified is tracked in
[ESTADO_MODULOS.md](../ESTADO_MODULOS.md); the language itself is in
[language.md](language.md).

> Orion does not copy Python. Each module is designed for a simple, fast API
> that needs no configuration.

## Modules at a glance

### Core
`fs` `json` `strings` `datetime` `random` `regex` `env` `process` `crypto` `term`

### System
`log` `config` `secret` `zip` `stream` `crypto2` `state`

### Network and web
`net` `ws` `router` `middleware` `sse` `proto` `browser` (and `serve`, a statement)

### Backend
`db` `auth` `cache` `session` `mail` `validate`

### Automation
`tarea` `cola` `chan` `watch`

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

## A tour with examples

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
translated = ai.translate(text, "english")
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
u = cosmos.run(u, 100)              -- cosmos.run(universe, steps?, dt?)
show cosmos.summary(u)
```

## By area

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

-- secret - secrets from the environment, NAME_FILE or .env, never printed
use "secret"

if not secret.production() {              -- ORION_ENV=production refuses .env
    secret.load()                          -- .env, only for development
}
cfg = secret.require(["DATABASE_URL", "JWT_KEY"])   -- lists every missing one at once
key = secret.require("JWT_KEY", { "min_length": 32 })
show "key: " + key                         -- "key: ***": show, log, errors and serve hide it
show secret.mask(key)                      -- "k3***9a"

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

> **Security note:** the `rsa` crate behind `rsa_decrypt` and `rsa_sign` is not
> constant-time (Marvin attack, RUSTSEC-2023-0071, no fixed version yet). Avoid
> decrypting or signing with RSA in a server where an attacker can send many
> requests and time the answers; `aes_encrypt` and `rsa_verify` are not affected.

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

-- router.match() can also be used manually. `match` is a keyword, so the
-- result cannot be bound to a variable of that name.
hit = router.match(r, "GET", "/users/42")
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
results = embed.search("When was it founded?", documents, 3)
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
results   = vector.search(db, query_vec, 5)
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
