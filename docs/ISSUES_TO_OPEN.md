# Issues to open

Copy each block into a new GitHub issue. Every one comes from a verified
finding, with the file to touch and a way to confirm the fix.

Delete this file once the issues exist.

---

## 1. Implement the `|>` pipe operator

**Labels:** `enhancement`, `help wanted`

The lexer produces `TokenKind::PipeOp` for `|>`, but the token appears **zero
times** in `parser.rs` and `codegen.rs`. Any program using it fails with:

```
Token inesperado en expresión: PipeOp
```

The README advertised a "Pipe operator" section until recently; it now
documents the operator as not implemented, which is accurate but a shame.

**Where:** `orion-vm/src/parser.rs`, `orion-vm/src/codegen.rs`

**Shape of the feature:** `x |> f(a)` should desugar to `f(x, a)`, so the piped
value becomes the first positional argument. That keeps it compatible with the
standard library, whose functions all take the data as their first parameter
(`excel.filter(data, ...)`, `frame.where_(f, ...)`).

**Done when:**

```orion
fn double(x) { return x * 2 }
show 5 |> double()      -- 10
```

runs, and a test covers chaining three calls.

---

## 2. Implement `excel.compute`

**Labels:** `enhancement`, `good first issue`

The API is designed and documented in the README (feature F-1) but does not
exist in the dispatcher. `orion --builtins-json` has no `excel.compute`.

**Where:** `orion-vm/src/modules/excel_mod.rs`

**Contract:** takes the data and a dict of `column name → lambda`. Each lambda
receives the whole row, so fields can reference each other. All columns are
computed in a single pass.

```orion
data = excel.compute(data, {
    "bonus":    row => row["sales"] * 0.05,
    "on_track": row => row["sales"] >= row["target"]
})
```

Remember the contract comment above the match arm (`// compute(data, cols) →
…`), or the generator will not pick the function up and
`registry_matches_runtime` will fail.

**Done when:** the README example runs and the roadmap table can say Complete
without lying.

---

## 3. Implement `excel.formula`

**Labels:** `enhancement`

Same situation as `compute`: documented as feature F-8, absent from the
dispatcher. The `excel.f` builder does exist, so part of the groundwork is
there.

Columns marked as formulas should stay live in the `.xlsx` and recalculate when
opened in Excel, rather than being flattened to values.

**Where:** `orion-vm/src/modules/excel_mod.rs`, next to `write_styled`

---

## 4. Allow `if` and `match` as expressions

**Labels:** `enhancement`

Both are statements today, so this fails:

```orion
tier = if sales > 90000 { "A" } else { "B" }
result = match value { 1 { "one" } _ { "other" } }
```

with `Token inesperado en expresión: If` / `Match`.

This is the single most common thing people expect and do not get. It also
forces awkward workarounds in lambdas, where an arrow body must become a block
with `return` just to branch.

**Where:** `orion-vm/src/parser.rs`, expression parsing

Worth discussing scope before starting: allowing it everywhere may create
ambiguity with block parsing. A narrower version — only in assignment and
`return` position — would already remove most of the friction.

---

## 5. Verify the documentation site's code examples

**Labels:** `documentation`, `good first issue`

The language repo has `readme_examples_parse` and the extension has
`check_snippets.js`, both of which compile every documented example against the
real compiler. The website has no equivalent, and it showed:

- `json.encrypt` and `json.decrypt`, which do not exist
- `trace_start` / `trace_end`, which do not exist
- `strip()`, which is called `trim`
- a loop over three users that only processed two, from a half-open range

**Where:** `Web-documentation-Orion`, `src/CodeShowCase.tsx` and
`src/components/docs/`

**Suggested approach:** move the examples out of JSX into real `.orx` files the
site imports as raw text, then run the same harness over that folder. Today
they are split across dozens of `<span>` elements, which is why no tool could
check them.

---

## 6. Extend the documentation harnesses from parsing to execution

**Labels:** `enhancement`, `help wanted`

The existing checks confirm that examples **parse**. That is not enough:
`db.buscar(...)`, `self.field` and `let x = 5` all parse cleanly and fail at
runtime with `no existe` / `Variable no definida`. Every one of those shipped in
our documentation.

**Where:** `orion-vm/tests/readme_examples_parse.rs`

**The hard part:** many examples open files, bind ports or need an API key, so
they cannot simply be run. The workable version is an opt-in marker — a
` ```orion run ` fence, or a list of block indices — for the subset that is
self-contained, executed with a timeout.

---

## 7. Make `orion check` run the type checker by default

**Labels:** `enhancement`, `question`

`orion check file.orx` only checks syntax. The type checker, which is what
catches calls to functions that do not exist, needs `--types`.

That default surprises people: `check` reporting "no errors" on a file that
crashes on the first line is worse than not running it. Note that running a
file already type-checks by default, so the CLI is inconsistent with itself.

Flipping the default would make `check` stricter. Worth deciding whether to
flip it, or keep `--types` and make the success message say clearly that types
were not verified.

**Where:** `orion-vm/src/main.rs`, the `--check` arm
