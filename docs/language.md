# The Orion language

Syntax reference with examples. The standard library has its own page,
[stdlib.md](stdlib.md), and the formal specification is [SPEC.md](../SPEC.md).

> **Note on naming.** Orion is written in English: keywords (`fn`, `return`,
> `if`, `while`, `shape`, `serve`) and the standard library alike, so
> `db.insert`, `cache.set` and `validate.required` are the canonical names.
> Orion was designed by a Spanish-speaking developer, and the Spanish names
> that came first still work as **deprecated aliases**: `db.insertar` runs
> today and will keep running for the rest of 0.1.x, but it is scheduled for
> removal. Write `db.insert` in new code. See `SPEC.md` section 11.

## Variables and types

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

## Data types

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

## Control flow

```orion
-- if / else if / else - the middle branch is two tokens, `else if`.
-- There is no `elsif` keyword.
if age >= 18 {
    show "Adult"
} else if age >= 13 {
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

-- for over a range - half-open: 1..10 covers 1 through 9
for x in 1..10 { show x }

-- for over a collection
for n in ["Ana", "Luis", "Eva"] { show n }

-- match is a statement, not an expression: each arm is `pattern { block }`,
-- with no `=>` arrow, and the whole thing cannot be assigned to a variable.
match value {
    1 { show "one" }
    2 { show "two" }
    _ { show "other" }
}

-- break / continue
for i in 1..100 {
    if i == 10 { break }
    if i % 2 == 0 { continue }
    show i
}
```

## Functions

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

## OOP - shapes

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

## Error handling

```orion
attempt {
    result = divide(10, 0)
    show result
} handle err {
    show "Error: " + err
}
```

## Native HTTP server

`serve` is a language statement: it takes a port and a handler function.
The handler receives the request and returns a dict with `status` and `body`.

```orion
use "db"

db.exec("app.db", "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT)")

fn router(req) {
    if req["path"] == "/ping" {
        return { "status": 200, "body": "pong" }
    }

    if req["path"] == "/users" {
        if req["method"] == "GET" {
            return { "status": 200, "body": db.query("app.db", "SELECT * FROM users") }
        }
        if req["method"] == "POST" {
            db.insert("app.db", "INSERT INTO users (name) VALUES (?)", [req["body"]])
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
[`router`](stdlib.md#block-b---modern-web-) module and pass its dispatcher to `serve`.

## Native AI - `think`, `learn`, `sense`

These call an external provider and need an API key. See the `llm` module for
explicit provider and model selection.

```orion
-- No module, no import: AI as a native statement
think "Summarize this text in 3 bullet points: " + content

-- The ai module for higher-level operations
use "ai" as ai

category  = ai.classify(email.text, ["spam", "work", "personal"])
-- Module functions take positional arguments only. Named arguments (`x = 1`)
-- work on functions you define, not on module methods.
summary   = ai.summarize(document)
translated = ai.translate(text, "english")
sentiment = ai.sentiment(review)   -- "positivo" / "negativo" / "neutro"
```

## Pipe operator

`|>` feeds the value on its left in as the **first** argument of the call on its
right. It is parser sugar: the result is the same `Call` you would have written
by hand, so the VM, the JIT and the type checker see nothing new.

```orion
result = data
    |> filter_by("active", yes)
    |> sort_by("date", "desc")
    |> top(10)

-- Equivalent to:
result = top(sort_by(filter_by(data, "active", yes), "date", "desc"), 10)
```

The right side can be a function name, a call, a method, or a lambda:

```orion
[1, 2, 3] |> len          -- 3
5 |> double               -- calls double(5)
5 |> add(10)              -- calls add(5, 10)
"  hi  " |> trim |> upper -- "HI"
3 |> (n) => n + 100       -- 103
```

Precedence sits between comparison and arithmetic, so both of these read the
way they look, without parentheses:

```orion
a + b |> f      -- f(a + b)
x |> len > 3    -- (x |> len) > 3
```

## Concurrency

```orion
-- Spawn (fire and forget)
spawn long_running_job()

-- Async/await
async fn process(item) { return item * 2 }
result = await process(21)
```
