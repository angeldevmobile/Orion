# Contributing to Orion

Thanks for taking a look. Orion is a beta built by one person, so there is
plenty of well-scoped work available and every fix lands quickly.

Licence is MIT. By contributing you agree your work ships under it.

## Building

```bash
git clone https://github.com/angeldevmobile/Orion.git
cd Orion
cargo build --release --manifest-path orion-vm/Cargo.toml
./orion-vm/target/release/orion --version
```

**Linux** needs a few system packages first (the same ones CI installs):

```bash
sudo apt-get install -y \
  libffi-dev libssl-dev libssh2-1-dev \
  libx11-dev libxcb-composite0-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xinerama0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev pkg-config
```

**macOS**: `brew install libssh2 libffi openssl@3 pkg-config`

**Windows**: MSVC Build Tools. Nothing else.

`node` is optional but recommended: `build.rs` uses it to regenerate the
builtins registry. Without it the build still succeeds using the committed
file, and prints a warning.

## Running the tests

```bash
cd orion-vm
cargo test --release          # unit, integration and differential tests
```

Language-level tests are written in Orion itself:

```bash
./orion-vm/target/release/orion test tests
```

## Three checks worth knowing about

These exist because each one caught a real problem that had gone unnoticed.
If you touch documentation or the standard library, they are the ones that
will fail on you.

| Test | What it protects |
|---|---|
| `readme_examples_parse` | Every ` ```orion ` block in the README must parse. Twelve of fifty-one did not. |
| `registry_matches_runtime` | The builtins registry must match the dispatcher in both directions. Twenty-two real functions were missing from it, which made the type checker reject working programs. |
| `check_snippets.js` (in the extension repo) | Every VS Code snippet must compile. Fifty-four of ninety-four did not: they were Rust and JavaScript syntax. |

## Adding a standard library function

1. Add the arm to the `match function` in `orion-vm/src/modules/<module>.rs`.
2. Put a contract comment above it: `// name(args) → description`. The
   generator reads that comment, so this is not optional decoration.
3. Rebuild. `scripts/gen_builtins.js` regenerates
   `src/cli/builtins_gen.rs`, which feeds `orion --builtins-json`, the editor
   autocompletion and the type checker.
4. Run `cargo test --release`. If `registry_matches_runtime` fails, the
   generator did not pick your function up — usually a name with uppercase or
   an accent that an over-narrow pattern skipped.

**Never edit `builtins_gen.rs` by hand.** It is regenerated on every build and
your changes will disappear.

## Adding a whole module

1. Create `orion-vm/src/modules/my_module.rs` with a
   `pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String>`.
2. Register it in `orion-vm/src/modules/mod.rs`, in **both** the dispatcher and
   `is_known_module`. Missing the second one makes the module load but the type
   checker reject it.
3. Add any crate dependency to `orion-vm/Cargo.toml`.

## Style

- Comments explain **why**, not what. The code already says what.
- Errors should say what to do next, not just what went wrong. Compare
  `db.querry() no existe` with `db.querry() no existe. ¿Quisiste decir db.query()?`
- New behaviour needs a test. Verify the test fails before your change: a test
  that never fails proves nothing.

## Quirks worth knowing before you write Orion

These trip up almost everyone, and several of them were wrong in our own docs
until recently:

- There is no `let`. Assignment declares: `x = 5`.
- The self-reference inside a `shape` is `me`, not `self`. Fields also work
  with no prefix at all.
- Comments are `--`, never `//`.
- Booleans are `yes` and `no`.
- Ranges are half-open: `0..3` covers 0, 1 and 2.
- The middle branch is `else if`, two tokens. There is no `elsif`.
- `match` is a statement, and its arms are `pattern { block }` with no `=>`.
  It cannot be assigned to a variable.
- `if` is a statement, not an expression.
- Two lambda forms: `fn(x) { block }` and `x => expression_or_block`. They do
  not mix — `fn x => ...` is a syntax error.
- Module functions take positional arguments only. Named arguments (`x = 1`)
  work on functions you define.
- `serve` takes a port and a **named** handler function. Anonymous lambdas
  cannot be dispatched, because each request runs in its own VM and handlers
  are looked up by name.

## Reporting a bug

Include the Orion version (`orion --version`), your platform, and the smallest
`.orx` file that reproduces it. If it involves the standard library, say
whether `orion check <file> --types` also reports it: the type checker catches
some things the parser does not.

## Publishing a package

```bash
orion --publish   # needs orion.json and ORION_GITHUB_TOKEN
```
