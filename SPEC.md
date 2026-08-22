# Orion Language Specification

Status: **draft, partial**. Version 0.1.3 of the compiler.

This document describes what the Orion compiler actually does, not what it is
meant to do. Every rule here was derived from `orion-vm/src/lexer.rs`,
`parser.rs`, `typechecker.rs` and `vm.rs`, and every executable claim is
exercised by `tests/spec_examples.orx`, which runs in CI. If this document and
the compiler disagree, that is a bug in one of them, and the disagreement is
reportable. Before this document existed, it was not.

Sections marked **Unspecified** are deliberate: the behaviour exists but has not
been pinned down yet, and may change without it counting as a breaking change.

---

## 1. Source text

Source files use the `.orx` extension and must be valid UTF-8. A leading byte
order mark (`U+FEFF`) is stripped and carries no meaning.

## 2. Comments

| Form | Meaning |
|---|---|
| `-- text` | Line comment, runs to end of line |
| `/// text` | Documentation comment, attached to the following declaration |
| `---` | **Error**: "Comentario inválido '---'. Usa '--' para comentarios" |
| `// text` | **Error**: "Comentario inválido '//'. Usa '--' para comentarios" |

`//` and `---` are rejected on purpose rather than ignored, so that a habit
brought from another language fails loudly instead of silently eating a line.
Note the ordering: `///` is tested before `//`, so a doc comment is a doc
comment and not an error.

## 3. Identifiers

An identifier starts with an ASCII letter or `_` and continues with ASCII
letters, digits or `_`.

Identifiers are **ASCII only**. `año` is not a valid identifier.

## 4. Keywords

Reserved and not usable as identifiers:

```
null undefined yes no and or not
int float bool string list dict any auto
show return break continue fn const for in if else while match use
attempt handle error as take extern
shape act using is on_create on_error me super
spawn async await
ask read write append serve with choices
think learn sense
```

Booleans are `yes` and `no`. There is no `true`/`false`, and no `let`.
The receiver inside a method is `me`, not `self` or `this`.

## 5. Literals

| Kind | Forms |
|---|---|
| Integer | Decimal (`42`), hexadecimal (`0x2A`), binary (`0b101010`) |
| Float | `3.14` — a digit is required on both sides of the dot. Exponent notation is supported: `1e3` is 1000, `1.5e2` is 150, `E` also works |
| Boolean | `yes`, `no` |
| Null | `null`, `undefined` |
| String | `"..."`, and triple-quoted `"""..."""` for multi-line |
| List | `[a, b, c]` |
| Dict | `{key: value}` — insertion order is preserved |

An empty `0x` or `0b` is an error, not zero.

There are **no digit separators and no octal literals**, and neither is
rejected by the lexer as such. `1_000` lexes as the integer `1` followed by
the identifier `_000`, and `0o17` as `0` followed by `o17`. Both end in an
error, but the error is about an undefined name and points past the number,
so it does not read as "that literal form does not exist".

### 5.1 Strings

Interpolation uses `${expr}`. Inside the braces the text is passed through
verbatim and parsed as an expression.

Escape sequences: `\uXXXX` (four hex digits) and `\xHH` (two hex digits) are
recognised. An unrecognised escape keeps the backslash and the character rather
than raising an error.

## 6. Operator precedence

From loosest to tightest binding. Every level is left-associative except
**power**, which is right-associative: `2 ** 3 ** 2` is `2 ** (3 ** 2)` = 512.

| # | Level | Operators |
|---|---|---|
| 1 | logical or | `or` |
| 2 | logical and | `and` |
| 3 | comparison | `==` `!=` `<` `<=` `>` `>=` |
| 4 | bitwise or | `\|` |
| 5 | bitwise xor | `^` |
| 6 | bitwise and | `&` |
| 7 | shift | `<<` `>>` |
| 8 | pipe | `\|>` |
| 9 | additive | `+` `-` |
| 10 | multiplicative | `*` `/` `%` |
| 11 | power | `**` |
| 12 | unary | `not` `-` |
| 13 | postfix | call, index, `.attr`, slice, `?.` |
| 14 | primary | literals, `(...)`, identifiers |

**Bitwise operators bind tighter than comparison**, unlike C. In C,
`a & b == c` parses as `a & (b == c)`, a classic source of bugs that compilers
warn about. In Orion it parses as `(a & b) == c`, which is how it reads.

### 6.1 The pipe operator

`|>` feeds the value on its left in as the **first argument** of the stage on
its right. It is pure syntax, resolved by the parser:

| Written | Becomes |
|---|---|
| `x \|> f` | `f(x)` |
| `x \|> f(a, b)` | `f(x, a, b)` |
| `x \|> obj.act(a)` | `obj.act(x, a)` |
| `x \|> mod.f` | `mod.f(x)` |
| `x \|> (n) => n * 3` | applies the lambda to `x` |

```orion
show 5 |> doble         -- 10
show 5 |> suma(3)       -- 8   (suma(5, 3))
p = 5 |> (n) => n * 3   -- 15
```

## 7. Name resolution

Inside a function body, a name is **local** if it is a parameter of the
function, or if the function assigns to it anywhere in its body. Any other name
resolves to the enclosing global.

This means assignment creates a local, so a function cannot overwrite a global
by assigning to it:

```orion
counter = 0
fn bump() {
    counter = counter + 1   -- creates a LOCAL named counter
}
bump()
show counter                -- still 0
```

Inside the body of an `act`, the fields of the shape are in scope directly.

`use "module"` defines a global. A function that calls a module therefore
depends on global resolution working, which is why this rule is load-bearing:
the native compiler once got it wrong and every real program broke.

## 8. Function values

A **lambda** is a closure. `type(fn(x) { return x })` is `fn`.

There are two syntaxes for one thing:

```orion
f = fn(x) { return x + 1 }   -- block form
g = (n) => n + 1             -- arrow form
h = n => n + 2               -- arrow form, single parameter, parentheses optional
```

A **named function** is represented at runtime as a string holding its own name,
and the call machinery resolves that name in the function table. This is
observable and has three consequences:

```orion
fn greet(x) { return "hi " + x }

show type(greet)          -- "string", not "fn"
show greet == "greet"     -- yes
s = "greet"
show s(2)                 -- "hi 2"  — a plain string is callable
```

Both forms are first class in the sense that matters: they can be stored,
passed and called, in every position, including inside a `${}` interpolation.

```orion
fn apply(f, v) { return f(v) }
show apply(greet, 10)                    -- works
show apply(fn(x) { return x + 1 }, 10)   -- works
```

But `type(f) == "fn"` is **not** a reliable test for callability, and a string
that happens to match a function name is indistinguishable from the function.
This is a leak of the implementation, recorded here because it is observable
today, not because it is intended. Changing it would be a breaking change.

## 9. Evaluation and semantics

Everything in this section was measured against the compiler, not assumed.

### 9.1 Evaluation order

Call arguments and dict literal values are evaluated **left to right**.

```orion
dos(a(), b())      -- a runs, then b
z = {x: c(), y: d()}   -- c runs, then d
```

### 9.2 Arithmetic

- Mixing `int` and `float` yields `float`: `type(1 + 1.0)` is `float`.
- `/` is **true division**, never integer division: `3 / 2` is `1.5`, a `float`.
- Integer overflow is an **error**, not a wraparound.
- `**` with an integer base and a negative integer exponent is an error
  ("Exponente negativo en potencia de enteros (usa flotantes)"). Use a float
  base: `2.0 ** -1` is `0.5`.

### 9.3 Null

`null` and `undefined` are **the same value at runtime**. `null == undefined`
is `yes`, and `type(undefined)` is `null`. The two spellings exist, the
distinction does not survive into execution.

### 9.4 Iteration

`for .. in` iterates lists. **It does not iterate a dict**: `for k in d` fails
with "GetIndex: tipo no soportado". Iterate the keys instead:

```orion
for k in d.keys() { ... }
```

Dicts preserve **insertion order**, and `keys()` returns them in that order.

## 10. Unspecified

Behaviour that exists but is not pinned down, and may change:

- Semantics of `think`, `learn` and `sense`. They perform a network call to a
  configured AI provider, so their result is provider-dependent by nature.
- Whether the error on a malformed numeric literal (see section 5) will keep
  reporting an undefined name rather than a bad literal.
- Unicode identifiers. Currently ASCII only; this is a lexer restriction, not a
  considered decision.
## 11. Stability contract

**Decided.** Takes effect with the 0.2 release:

- The canonical name of every standard library function is its **English**
  name. This is the name the registry documents, the name hover and
  autocompletion offer, and the name the reference site lists.
- The Spanish names are **deprecated aliases**. They still work today and
  will keep working for the rest of 0.1.x, but they are scheduled for
  removal in a future version and should not be used in new code. They are
  not part of the stable surface: `db.insert` is, `db.insertar` is not.
  `orion check` reports every use of one, with the English name to replace
  it, so nobody has to find out on the day it is removed.
- Runtime type names as returned by `type(x)` are stable:
  `int` `float` `string` `bool` `list` `dict` `ptr` `null` `fn` `task`
  `module<...>`, or the shape name for an instance. Note that `fn` covers
  lambdas only; see section 8 for named functions.
- Keywords are stable within a major version.
- Anything listed as **Unspecified** (section 10) is not covered.
