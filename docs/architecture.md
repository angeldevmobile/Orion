# Architecture

How Orion is built: one compiler front end, three execution engines. For the
measured performance of each engine, see the README.

## Pipeline

Orion is **not a tree-walking interpreter** - that legacy was removed. It is a
bytecode compiler with **three execution backends** that share one frontend and
produce identical results, verified by differential tests. Around 55,000 lines
of Rust, including the 61 native modules.

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
    ├──►  jit/  (--jit)       ← JIT to machine code via Cranelift. Numbers live
    │                            inside the value (NaN-boxing) and arithmetic is
    │                            compiled inline. Falls back to the VM
    │                            automatically when an instruction is not yet
    │                            supported in the JIT.
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

## Component status

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
| Standard library modules | ✅ 61 modules | Rust |
| Cloud native (S3 / SSH / Docker) | ✅ Complete | Rust |
| Full CLI | ✅ Complete | Rust |
| VS Code extension (published on the Marketplace) | ✅ Complete | TypeScript |
