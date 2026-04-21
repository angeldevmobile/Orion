#!/usr/bin/env python3
# ╔══════════════════════════════════════════════════════╗
# ║               ORION CLI — Cognitive Runtime           ║
# ╚══════════════════════════════════════════════════════╝
import argparse
import sys
import os
from rich.console import Console, Group
from rich.panel import Panel
from rich.text import Text
from rich.table import Table
from rich.prompt import Prompt
from pyfiglet import Figlet

sys.path.append(os.path.dirname(os.path.dirname(__file__)))
from core.lexer import lex
from core.parser import parse
from core.eval import evaluate

console = Console(width=120)
VERSION = "1.1.0-alpha"


# ╔══════════════════════════════════════════════════════╗
# ║                 SECCIÓN: INTERFAZ BASE CLI           ║
# ╚══════════════════════════════════════════════════════╝
def banner():
    """Muestra el banner con gradiente"""
    fig = Figlet(font="ansi_shadow")
    logo = fig.renderText("> ORION")

    # Gradiente suave azul-celeste
    colors = ["#00FFFF", "#00E0FF", "#00C0FF", "#0090FF", "#0060FF", "#0040D0"]
    gradient_logo = Text()
    for i, line in enumerate(logo.splitlines()):
        color = colors[min(i // 2, len(colors) - 1)]
        gradient_logo.append(line + "\n", style=f"bold {color}")

    # Tips iniciales
    tips = [
        "[bold white]Tips for getting started:[/bold white]",
        "1. Escribe código, ejecuta archivos o usa el modo interactivo.",
        "2. Sé específico para obtener mejores resultados.",
        "3. Usa [cyan]/help[/cyan] para más información.",
    ]

    main_panel = Panel.fit(
        Group(gradient_logo, "\n", "\n".join(tips)),
        border_style="bright_cyan",
        padding=(1, 3),
    )
    console.print(main_panel)

    console.print(
        "\n> [dim]Escribe un comando o programa Orion para ejecutar[/dim]\n",
        style="italic bright_white",
    )


def run_script(path: str):
    """Ejecuta un archivo Orion .orx vía compilador + VM Rust"""
    import subprocess
    if not os.path.exists(path):
        console.print(f"[red]Archivo no encontrado:[/red] {path}")
        sys.exit(1)

    console.print(f"[blue]Ejecutando:[/blue] {path}\n")

    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    orbc_path = os.path.splitext(path)[0] + ".orbc"
    vm_exe = os.path.join(project_root, "orion-vm", "target", "release", "orion.exe")

    try:
        from compiler.bytecode_compiler import compile_file
        compile_file(path, orbc_path)
    except Exception as e:
        console.print(f"[bold red]Error de compilación:[/bold red] {e}")
        sys.exit(1)

    result = subprocess.run([vm_exe, orbc_path])
    if result.returncode != 0:
        sys.exit(result.returncode)


def _repl_read_block() -> str | None:
    """
    Lee una línea o bloque multi-línea.
    Detecta llaves `{}` abiertas para continuar pidiendo input.
    Retorna None si el usuario cancela (Ctrl+C / Ctrl+D).
    """
    lines = []
    depth = 0

    while True:
        try:
            if depth == 0:
                prompt = "[bold cyan]orion[/bold cyan] [dim]»[/dim] "
            else:
                pad = "  " * depth
                prompt = f"[dim]{pad}...[/dim] "
            line = Prompt.ask(prompt, default="")
        except (EOFError, KeyboardInterrupt):
            return None

        lines.append(line)
        depth += line.count("{") - line.count("}")

        # Salir del bucle cuando la profundidad cierra y la última línea no es vacía,
        # o cuando no hay bloque abierto desde el inicio
        if depth <= 0:
            break

    return "\n".join(lines)


def _is_expression_node(node) -> bool:
    """
    True si el nodo es una expresión que tiene sentido auto-imprimir.
    El parser envuelve expresiones en nivel de statement como ('EXPR', inner, line).
    """
    if not isinstance(node, tuple):
        return False
    tag = node[0]
    # El parser envuelve expresiones standalone en ('EXPR', inner_expr, line)
    if tag == "EXPR":
        inner = node[1]
        return _is_expression_node(inner)
    # Nodos de statement → no auto-imprimir
    _STMT_TAGS = {
        "ASSIGN", "IF", "WHILE", "FOR", "FN", "ASYNC_FN", "RETURN",
        "SPAWN", "AWAIT_STMT", "THINK", "LEARN", "SENSE",
        "READ_STMT", "WRITE_STMT", "SHAPE_DEF", "USE",
    }
    if tag in _STMT_TAGS:
        return False
    # show/print como CALL son side-effects → no auto-imprimir
    if tag == "CALL":
        fn = node[1]
        fn_name = fn[1] if isinstance(fn, tuple) else fn
        if str(fn_name).lower() in ("show", "print"):
            return False
    _EXPR_TAGS = {
        "BINARY_OP", "UNARY_OP", "LIST", "DICT", "INDEX",
        "CALL", "CALL_METHOD", "ATTR_ACCESS", "IS_CHECK",
        "AWAIT_EXPR", "IDENT",
    }
    return tag in _EXPR_TAGS


def _unwrap_expr_node(node):
    """Desenvuelve el nodo EXPR para obtener la expresión interna."""
    if isinstance(node, tuple) and node[0] == "EXPR":
        return node[1]
    return node


def _repl_format_value(val) -> str:
    """Formatea un valor para mostrarlo en el REPL."""
    if val is None:
        return "null"
    t = type(val).__name__
    s = str(val)
    if isinstance(val, str):
        return f'"{val}"'
    if isinstance(val, bool):
        return "yes" if val else "no"
    if isinstance(val, (list, dict)):
        return s
    return s


def _repl_show_vars(variables: dict):
    """Muestra las variables del scope actual."""
    # Excluir variables internas de Orion
    _internal = {"null", "yes", "no", "clusters", "SPREADSHEET", "COSMOS",
                 "CRYPTO", "INSIGHT"}
    user_vars = {k: v for k, v in variables.items() if k not in _internal}
    if not user_vars:
        console.print("  [dim](sin variables)[/dim]")
        return
    t = Table(show_header=True, header_style="bold cyan", border_style="dim blue",
              box=None, pad_edge=False)
    t.add_column("nombre", style="cyan", width=16)
    t.add_column("tipo",   style="dim",  width=10)
    t.add_column("valor",  style="white")
    for k, v in sorted(user_vars.items()):
        type_name = type(v).__name__
        val_str = _repl_format_value(v)
        if len(val_str) > 60:
            val_str = val_str[:57] + "..."
        t.add_row(k, type_name, val_str)
    console.print(t)


def _repl_show_fns(functions: dict):
    """Muestra las funciones definidas."""
    if not functions:
        console.print("  [dim](sin funciones)[/dim]")
        return
    t = Table(show_header=True, header_style="bold cyan", border_style="dim blue",
              box=None, pad_edge=False)
    t.add_column("función", style="cyan", width=18)
    t.add_column("parámetros", style="white")
    for name, defs in sorted(functions.items()):
        if isinstance(defs, list) and defs:
            fn = defs[0]
            params = ", ".join(fn.get("params", []) if isinstance(fn, dict) else [])
        else:
            params = "?"
        t.add_row(name, f"({params})")
    console.print(t)


_REPL_HELP = """\
[bold cyan]Comandos del REPL:[/bold cyan]
  [cyan]:vars[/cyan]          — muestra variables actuales
  [cyan]:fns[/cyan]           — muestra funciones definidas
  [cyan]:clear[/cyan]         — limpia el estado (vars + fns)
  [cyan]:reset[/cyan]         — alias de :clear
  [cyan]:help[/cyan]          — muestra esta ayuda
  [cyan]:exit[/cyan]  [dim]exit[/dim]    — sale del REPL

[bold cyan]Sintaxis rápida:[/bold cyan]
  [white]x = 42[/white]              — asignar variable
  [white]show x[/white]              — imprimir valor
  [white]x + y[/white]               — expresión (auto-imprime resultado)
  [white]fn suma(a,b) { ... }[/white] — definir función  [dim](multi-línea)[/dim]
  [white]if cond { ... }[/white]      — condicional      [dim](multi-línea)[/dim]"""


def repl():
    """Modo interactivo Orion REPL con soporte multi-línea."""
    from core.eval import eval_expr

    console.print(
        Panel(
            "[bold cyan]Orion REPL[/bold cyan] [dim]— Entorno interactivo[/dim]\n"
            "[dim]Escribe [/dim][cyan]:help[/cyan][dim] para ver comandos. "
            "[/dim][cyan]:exit[/cyan][dim] para salir.[/dim]",
            border_style="bright_cyan",
            padding=(0, 2),
        )
    )

    variables: dict = {}
    functions: dict = {}

    # Pre-inicializar silenciosamente (registra null, yes, no, etc.)
    import io as _io
    from contextlib import redirect_stdout, redirect_stderr
    with redirect_stdout(_io.StringIO()), redirect_stderr(_io.StringIO()):
        try:
            evaluate([], variables, functions)
        except Exception:
            pass

    while True:
        try:
            src = _repl_read_block()
            if src is None:
                console.print("\n[dim]Saliendo…[/dim]")
                break

            stripped = src.strip()
            if not stripped:
                continue

            # ── Comandos especiales ────────────────────────────────────────
            cmd = stripped.lower()
            if cmd in ("exit", "quit", "salir", ":exit", ":quit"):
                break
            if cmd in (":vars",):
                _repl_show_vars(variables)
                continue
            if cmd in (":fns",):
                _repl_show_fns(functions)
                continue
            if cmd in (":clear", ":reset"):
                variables.clear()
                functions.clear()
                with redirect_stdout(_io.StringIO()), redirect_stderr(_io.StringIO()):
                    try:
                        evaluate([], variables, functions)
                    except Exception:
                        pass
                console.print("  [dim]Estado limpiado.[/dim]")
                continue
            if cmd in (":help",):
                console.print(_REPL_HELP)
                continue

            # ── Compilar y ejecutar ────────────────────────────────────────
            tokens = lex(stripped)
            ast = parse(tokens)

            # Auto-print: si es un único nodo de expresión, evaluar y mostrar
            if len(ast) == 1 and _is_expression_node(ast[0]):
                inner = _unwrap_expr_node(ast[0])
                val = eval_expr(inner, variables, functions)
                if val is not None:
                    console.print(f"  [bold green]=> {_repl_format_value(val)}[/bold green]")
            else:
                evaluate(ast, variables, functions)

        except KeyboardInterrupt:
            console.print("\n  [dim]Interrumpido. Escribe :exit para salir.[/dim]")
            continue
        except EOFError:
            break
        except Exception as e:
            console.print(f"  [bold red]![/bold red] [red]{e}[/red]")


def show_help():
    """Tabla de ayuda tipo Gemini CLI"""
    help_table = Table(
        title="[bold magenta]Orion CLI — Comandos disponibles[/bold magenta]",
        show_header=True,
        header_style="bold magenta",
        border_style="blue",
    )

    help_table.add_column("Comando", style="cyan", width=30)
    help_table.add_column("Descripción", style="white")
    help_table.add_column("Ejemplo", style="green")

    help_table.add_row("orion <archivo.orx>", "Ejecuta un archivo Orion", "orion programa.orx")
    help_table.add_row("orion repl", "Inicia el entorno interactivo REPL", "orion repl")
    help_table.add_row("", "", "")
    help_table.add_row("orion new <proyecto>", "Crea un proyecto backend nuevo", "orion new mi-api")
    help_table.add_row("orion build <archivo>", "Compila a .orbc sin ejecutar", "orion build main.orx")
    help_table.add_row("orion check <archivo>", "Verifica sintaxis sin ejecutar", "orion check main.orx")
    help_table.add_row("orion watch <archivo>", "Hot reload — re-ejecuta al guardar", "orion watch main.orx")
    help_table.add_row("orion bench <archivo>", "Benchmark de ejecución", "orion bench main.orx --runs=20")
    help_table.add_row("orion test [carpeta]", "Ejecuta todos los tests", "orion test")
    help_table.add_row("orion doctor", "Verifica el entorno de desarrollo", "orion doctor")
    help_table.add_row("", "", "")
    help_table.add_row("orion add <paquete>", "Instala un paquete", "orion add math")
    help_table.add_row("orion remove <paquete>", "Desinstala un paquete", "orion remove math")
    help_table.add_row("orion list", "Lista los paquetes instalados", "orion list")
    help_table.add_row("orion search <query>", "Busca en el registry", "orion search strings")
    help_table.add_row("orion update [paquete]", "Actualiza uno o todos los paquetes", "orion update")
    help_table.add_row("", "", "")
    help_table.add_row("--visual", "Inicia el modo visual animado", "orion --visual")
    help_table.add_row("--version", "Muestra la versión de Orion", "orion --version")
    help_table.add_row("--help", "Muestra esta ayuda", "orion --help")

    console.print(help_table)


# ╔══════════════════════════════════════════════════════╗
# ║     SECCIÓN: orion doctor / watch / bench / test     ║
# ╚══════════════════════════════════════════════════════╝

def cmd_doctor():
    """orion doctor — verifica el entorno de desarrollo Orion"""
    import shutil, subprocess, platform

    console.print("\n[bold cyan]Orion Doctor[/bold cyan]  [dim]— verificando entorno...[/dim]\n")

    checks = []

    # ── Python ──────────────────────────────────────────────────────────────
    pv = sys.version_info
    py_ok = pv >= (3, 10)
    checks.append((
        "Python >= 3.10",
        py_ok,
        f"{pv.major}.{pv.minor}.{pv.micro}",
        "Instala Python 3.10+ desde python.org" if not py_ok else "",
    ))

    # ── Rust / cargo ────────────────────────────────────────────────────────
    cargo = shutil.which("cargo")
    cargo_ver = ""
    if cargo:
        try:
            r = subprocess.run(["cargo", "--version"], capture_output=True, text=True, timeout=5)
            cargo_ver = r.stdout.strip()
        except Exception:
            cargo_ver = "instalado"
    checks.append((
        "Rust / Cargo",
        bool(cargo),
        cargo_ver or "no encontrado",
        "Instala desde https://rustup.rs" if not cargo else "",
    ))

    # ── orion-vm binary ─────────────────────────────────────────────────────
    vm_debug   = os.path.join(os.path.dirname(os.path.dirname(__file__)), "orion-vm", "target", "debug", "orion.exe")
    vm_release = os.path.join(os.path.dirname(os.path.dirname(__file__)), "orion-vm", "target", "release", "orion.exe")
    vm_found   = os.path.exists(vm_debug) or os.path.exists(vm_release)
    vm_path    = vm_release if os.path.exists(vm_release) else (vm_debug if os.path.exists(vm_debug) else "no encontrado")
    checks.append((
        "Orion VM (orion.exe)",
        vm_found,
        os.path.basename(os.path.dirname(vm_path)) if vm_found else "no compilado",
        "Ejecuta: cd orion-vm && cargo build --release" if not vm_found else "",
    ))

    # ── Dependencias Python ──────────────────────────────────────────────────
    _deps = {
        "rich":     "Interfaz CLI (requerido)",
        "pyfiglet": "Banner ASCII (requerido)",
        "requests": "HTTP client (opcional)",
        "httpx":    "HTTP async client (opcional)",
        "watchdog": "orion watch (opcional)",
    }
    for dep, desc in _deps.items():
        try:
            __import__(dep)
            ok = True
            msg = "instalado"
        except ImportError:
            ok = "opcional" in desc
            msg = "no instalado"
        checks.append((
            f"pip: {dep}",
            ok,
            msg,
            f"pip install {dep}" if not ok or msg == "no instalado" else "",
        ))

    # ── Compiler ─────────────────────────────────────────────────────────────
    compiler = os.path.join(os.path.dirname(os.path.dirname(__file__)), "compiler", "bytecode_compiler.py")
    checks.append((
        "Bytecode compiler",
        os.path.exists(compiler),
        "compiler/bytecode_compiler.py",
        "Falta el compilador" if not os.path.exists(compiler) else "",
    ))

    # ── Mostrar tabla ─────────────────────────────────────────────────────────
    t = Table(show_header=True, header_style="bold cyan", border_style="dim", pad_edge=True)
    t.add_column("Componente",    style="white",  width=26)
    t.add_column("Estado",        width=8)
    t.add_column("Detalle",       style="dim",    width=36)
    t.add_column("Fix",           style="yellow", width=44)

    issues = 0
    for name, ok, detail, fix in checks:
        if ok is True:
            badge = "[bold green]  ✔ OK [/bold green]"
        elif ok is False:
            badge = "[bold red]  ✖ ERR[/bold red]"
            issues += 1
        else:
            badge = "[bold yellow]  ~ OPT[/bold yellow]"
        t.add_row(name, badge, detail, fix)

    console.print(t)
    console.print()

    if issues == 0:
        console.print("[bold green]✔ Entorno listo.[/bold green]  Orion está correctamente configurado.\n")
    else:
        console.print(f"[bold red]✖ {issues} problema(s) encontrado(s).[/bold red]  Revisa los fixes arriba.\n")


def cmd_watch(path: str):
    """orion watch <archivo.orx> — re-ejecuta al detectar cambios"""
    import pathlib, time, subprocess

    src = pathlib.Path(path)
    if not src.exists():
        console.print(f"[red]Archivo no encontrado: {path}[/red]")
        return

    compiler = os.path.join(os.path.dirname(os.path.dirname(__file__)), "compiler", "bytecode_compiler.py")
    vm_path   = os.path.join(os.path.dirname(os.path.dirname(__file__)), "orion-vm", "target", "debug", "orion.exe")
    orbc      = src.with_suffix(".orbc")

    console.print(
        Panel(
            f"[bold cyan]Orion Watch[/bold cyan]  [dim]— observando[/dim] [cyan]{src}[/cyan]\n"
            "[dim]Ctrl+C para detener[/dim]",
            border_style="cyan",
            padding=(0, 2),
        )
    )

    def run_once(stamp: str):
        console.rule(f"[dim]{stamp}[/dim]")
        r = subprocess.run([sys.executable, compiler, str(src)], capture_output=True, text=True)
        if r.returncode != 0:
            console.print(f"[red]✖ Compile error:[/red]\n{r.stderr or r.stdout}")
            return
        if os.path.exists(vm_path):
            subprocess.run([vm_path, str(orbc)])
        else:
            # fallback: Python eval
            try:
                source = src.read_text(encoding="utf-8")
                tokens = lex(source)
                ast    = parse(tokens)
                variables, functions = {}, {}
                evaluate(ast, variables, functions)
            except Exception as e:
                console.print(f"[red]✖ {e}[/red]")

    last_mtime = None
    try:
        while True:
            mtime = src.stat().st_mtime
            if mtime != last_mtime:
                last_mtime = mtime
                stamp = time.strftime("%H:%M:%S")
                run_once(stamp)
            time.sleep(0.5)
    except KeyboardInterrupt:
        console.print("\n[dim]Watch detenido.[/dim]")


def cmd_bench(path: str, runs: int = 10):
    """orion bench <archivo.orx> — mide tiempo de ejecución (N corridas)"""
    import pathlib, time, statistics, subprocess

    src = pathlib.Path(path)
    if not src.exists():
        console.print(f"[red]Archivo no encontrado: {path}[/red]")
        return

    compiler = os.path.join(os.path.dirname(os.path.dirname(__file__)), "compiler", "bytecode_compiler.py")
    vm_path   = os.path.join(os.path.dirname(os.path.dirname(__file__)), "orion-vm", "target", "debug", "orion.exe")
    orbc      = src.with_suffix(".orbc")

    # Compilar una sola vez
    r = subprocess.run([sys.executable, compiler, str(src)], capture_output=True, text=True)
    if r.returncode != 0:
        console.print(f"[red]✖ Compile error:[/red]\n{r.stderr or r.stdout}")
        return

    console.print(f"\n[bold cyan]Benchmarking[/bold cyan] [white]{src}[/white]  [dim]({runs} corridas)[/dim]\n")

    times_ms = []
    use_vm   = os.path.exists(vm_path)

    with console.status("[cyan]Ejecutando...[/cyan]", spinner="dots"):
        for _ in range(runs):
            t0 = time.perf_counter()
            if use_vm:
                subprocess.run([vm_path, str(orbc)], capture_output=True)
            else:
                try:
                    source = src.read_text(encoding="utf-8")
                    tokens = lex(source)
                    ast    = parse(tokens)
                    evaluate(ast, {}, {})
                except Exception:
                    pass
            times_ms.append((time.perf_counter() - t0) * 1000)

    mean  = statistics.mean(times_ms)
    med   = statistics.median(times_ms)
    stdev = statistics.stdev(times_ms) if len(times_ms) > 1 else 0
    best  = min(times_ms)
    worst = max(times_ms)

    t = Table(show_header=True, header_style="bold cyan", border_style="dim", pad_edge=True)
    t.add_column("Métrica",  style="cyan",  width=16)
    t.add_column("Tiempo",   style="white", width=14)
    t.add_column("",         style="dim",   width=30)

    def bar(val, max_val, width=20):
        filled = int((val / max_val) * width)
        return "█" * filled + "░" * (width - filled)

    t.add_row("Promedio",  f"{mean:.2f} ms",  bar(mean,  worst))
    t.add_row("Mediana",   f"{med:.2f} ms",   bar(med,   worst))
    t.add_row("Mejor",     f"[green]{best:.2f} ms[/green]",  bar(best,  worst))
    t.add_row("Peor",      f"[red]{worst:.2f} ms[/red]",    bar(worst, worst))
    t.add_row("Std dev",   f"{stdev:.2f} ms", "")
    t.add_row("Corridas",  str(runs),         f"usando {'VM Rust' if use_vm else 'Python eval'}")

    console.print(t)
    console.print()


def cmd_test(folder: str = "."):
    """orion test [carpeta] — descubre y ejecuta tests/*.orx automáticamente"""
    import pathlib, subprocess, time

    base = pathlib.Path(folder)
    # Buscar archivos test en test/ o con prefijo test_
    test_files = sorted(
        list(base.rglob("test/*.orx")) +
        list(base.rglob("test_*.orx"))
    )
    # deduplicar
    seen = set()
    unique = []
    for f in test_files:
        if str(f) not in seen:
            seen.add(str(f))
            unique.append(f)
    test_files = unique

    if not test_files:
        console.print(f"[yellow]No se encontraron archivos de test en '{folder}'[/yellow]")
        console.print("[dim]Crea archivos con prefijo test_ o dentro de una carpeta test/[/dim]")
        return

    compiler = os.path.join(os.path.dirname(os.path.dirname(__file__)), "compiler", "bytecode_compiler.py")
    vm_path   = os.path.join(os.path.dirname(os.path.dirname(__file__)), "orion-vm", "target", "debug", "orion.exe")

    console.print(f"\n[bold cyan]Orion Test Runner[/bold cyan]  [dim]— {len(test_files)} archivo(s) encontrado(s)[/dim]\n")

    results = []
    passed = failed = 0

    for tf in test_files:
        t0 = time.perf_counter()
        # Compilar
        rc = subprocess.run([sys.executable, compiler, str(tf)], capture_output=True, text=True)
        if rc.returncode != 0:
            elapsed = (time.perf_counter() - t0) * 1000
            results.append((str(tf), "COMPILE_ERR", elapsed, rc.stderr or rc.stdout))
            failed += 1
            continue

        # Ejecutar
        orbc = tf.with_suffix(".orbc")
        if os.path.exists(vm_path):
            rx = subprocess.run([vm_path, str(orbc)], capture_output=True, text=True, timeout=10)
            ok = rx.returncode == 0
            out = (rx.stdout + rx.stderr).strip()
        else:
            ok = True
            out = ""
            try:
                source = tf.read_text(encoding="utf-8")
                tokens = lex(source)
                ast    = parse(tokens)
                evaluate(ast, {}, {})
            except Exception as e:
                ok = False
                out = str(e)

        elapsed = (time.perf_counter() - t0) * 1000
        status = "PASS" if ok else "FAIL"
        results.append((str(tf), status, elapsed, out if not ok else ""))
        if ok:
            passed += 1
        else:
            failed += 1

    # Tabla de resultados
    t = Table(show_header=True, header_style="bold cyan", border_style="dim", pad_edge=True)
    t.add_column("Archivo",  style="white", width=40)
    t.add_column("Estado",   width=10)
    t.add_column("Tiempo",   style="dim",   width=12)
    t.add_column("Info",     style="dim",   width=40)

    for name, status, ms, info in results:
        short = name.replace(folder + os.sep, "") if folder != "." else name
        if status == "PASS":
            badge = "[bold green]  ✔ PASS[/bold green]"
        elif status == "COMPILE_ERR":
            badge = "[bold yellow]  ~ CERR[/bold yellow]"
        else:
            badge = "[bold red]  ✖ FAIL[/bold red]"
        t.add_row(short, badge, f"{ms:.1f} ms", (info[:38] + "…") if len(info) > 38 else info)

    console.print(t)
    console.print()

    total = passed + failed
    if failed == 0:
        console.print(f"[bold green]✔ {passed}/{total} tests pasaron.[/bold green]\n")
    else:
        console.print(f"[bold red]✖ {failed}/{total} tests fallaron.[/bold red]  {passed} pasaron.\n")


# ╔══════════════════════════════════════════════════════╗
# ║         SECCIÓN: orion new / build / check           ║
# ╚══════════════════════════════════════════════════════╝

# ── Plantillas del scaffold ────────────────────────────────────────────────────

_SCAFFOLD_MAIN = '''\
-- {name} — Orion Backend
-- Generated by: orion new {name}
-- Run:   orion {name}/main.orx
-- Build: orion build {name}/main.orx

use net
use json

-- ─────────────────────────────────────────────
-- Configuration
-- ─────────────────────────────────────────────
PORT = 8080
APP_NAME = "{name}"

-- ─────────────────────────────────────────────
-- Helpers
-- ─────────────────────────────────────────────
fn ok_response(data) {{
    return {{ "status": 200, "body": data }}
}}

fn error_response(code, message) {{
    return {{ "status": code, "body": message }}
}}

-- ─────────────────────────────────────────────
-- In-memory store (replace with DB later)
-- ─────────────────────────────────────────────
users = [
    {{ "id": 1, "name": "Alice", "email": "alice@example.com" }},
    {{ "id": 2, "name": "Bob",   "email": "bob@example.com"   }}
]

-- ─────────────────────────────────────────────
-- Router — handles every incoming request
-- ─────────────────────────────────────────────
fn router(req) {{
    path   = req["path"]
    method = req["method"]

    -- Health check
    if path == "/ping" {{
        return ok_response("pong")
    }}

    -- App info
    if path == "/" {{
        return ok_response("Welcome to " + APP_NAME + " — Orion Backend")
    }}

    -- Users resource
    if path == "/users" {{
        if method == "GET" {{
            return ok_response(str(users))
        }}
        if method == "POST" {{
            body   = req["body"]
            new_id = len(users) + 1
            return ok_response("User created with id " + str(new_id))
        }}
    }}

    return error_response(404, "Not found: " + path)
}}

-- ─────────────────────────────────────────────
-- Start server
-- ─────────────────────────────────────────────
show "Starting " + APP_NAME + " on port " + str(PORT)
show "  GET  /ping          — health check"
show "  GET  /              — app info"
show "  GET  /users         — list users"
show "  POST /users         — create user"
serve PORT router
'''

_SCAFFOLD_ENV = '''\
# Environment variables for {name}
# Copy this file to .env and fill in real values

APP_NAME={name}
PORT=8080
ENV=development

# Database (when ready)
# DB_URL=postgres://user:pass@localhost:5432/{name}

# Auth (when ready)
# JWT_SECRET=change-me-in-production
'''

_SCAFFOLD_ORION_JSON = '''\
{{
  "name": "{name}",
  "version": "0.1.0",
  "description": "An Orion backend project",
  "main": "main.orx",
  "author": "",
  "license": "MIT",
  "dependencies": {{}}
}}
'''

_SCAFFOLD_GITIGNORE = '''\
# Orion
*.orbc

# Environment
.env

# Python
__pycache__/
*.py[cod]
*.egg-info/

# OS
.DS_Store
Thumbs.db
'''


def cmd_new(project_name: str):
    """orion new <proyecto> — scaffold a new backend project"""
    import pathlib

    base = pathlib.Path(project_name)

    if base.exists():
        console.print(f"[red]Error: La carpeta '{project_name}' ya existe.[/red]")
        return

    # Directorios
    dirs = [base, base / "lib", base / "test"]
    for d in dirs:
        d.mkdir(parents=True, exist_ok=True)

    # Archivos
    files = {
        base / "main.orx":      _SCAFFOLD_MAIN.format(name=project_name),
        base / ".env.example":  _SCAFFOLD_ENV.format(name=project_name),
        base / "orion.json":    _SCAFFOLD_ORION_JSON.format(name=project_name),
        base / ".gitignore":    _SCAFFOLD_GITIGNORE,
        base / "lib" / "utils.orx": (
            f"-- {project_name}/lib/utils.orx — shared utilities\n\n"
            "fn clamp(val, min_val, max_val) {\n"
            "    if val < min_val { return min_val }\n"
            "    if val > max_val { return max_val }\n"
            "    return val\n"
            "}\n\n"
            "fn is_empty(s) {\n"
            "    return len(s) == 0\n"
            "}\n"
        ),
        base / "test" / "test_routes.orx": (
            f"-- {project_name}/test/test_routes.orx — basic route tests\n\n"
            'use net\n\n'
            'BASE = "http://localhost:8080"\n\n'
            'res = net.reach(BASE + "/ping")\n'
            'show "ping: " + str(res.status)\n\n'
            'res = net.reach(BASE + "/users")\n'
            'show "users: " + str(res.status)\n'
        ),
    }

    for path, content in files.items():
        path.write_text(content, encoding="utf-8")

    # Output
    console.print(
        Panel(
            f"[bold green]Proyecto '[cyan]{project_name}[/cyan]' creado exitosamente![/bold green]\n\n"
            f"[white]Estructura:[/white]\n"
            f"  [cyan]{project_name}/[/cyan]\n"
            f"  ├── [white]main.orx[/white]          — servidor backend\n"
            f"  ├── [white]orion.json[/white]        — manifiesto del proyecto\n"
            f"  ├── [white].env.example[/white]      — variables de entorno\n"
            f"  ├── [white].gitignore[/white]\n"
            f"  ├── [white]lib/utils.orx[/white]     — utilidades compartidas\n"
            f"  └── [white]test/test_routes.orx[/white] — tests básicos\n\n"
            f"[dim]Para iniciar:[/dim]\n"
            f"  [cyan]orion {project_name}/main.orx[/cyan]",
            border_style="green",
            title="[bold]orion new[/bold]",
        )
    )


# ╔══════════════════════════════════════════════════════╗
# ║              SECCIÓN: PACKAGE MANAGER                ║
# ╚══════════════════════════════════════════════════════╝
def cmd_build(path: str):
    """orion build <archivo.orx> — compila a .orbc sin ejecutar"""
    import pathlib
    src = pathlib.Path(path)
    if not src.exists():
        console.print(f"[red]Archivo no encontrado: {path}[/red]")
        return
    if src.suffix != ".orx":
        console.print(f"[red]Se esperaba un archivo .orx, no {src.suffix}[/red]")
        return

    import subprocess, time
    compiler = os.path.join(os.path.dirname(os.path.dirname(__file__)), "compiler", "bytecode_compiler.py")
    t0 = time.perf_counter()
    result = subprocess.run(
        [sys.executable, compiler, str(src)],
        capture_output=True, text=True
    )
    elapsed = (time.perf_counter() - t0) * 1000

    if result.returncode == 0:
        out_path = src.with_suffix(".orbc")
        console.print(
            f"[green]✔[/green] Compilado: [cyan]{src}[/cyan] → [cyan]{out_path}[/cyan]  "
            f"[dim]({elapsed:.1f} ms)[/dim]"
        )
    else:
        console.print(f"[red]✖ Error al compilar:[/red]\n{result.stderr or result.stdout}")


def cmd_check(path: str):
    """orion check <archivo.orx> — verifica sintaxis sin ejecutar"""
    import pathlib
    src = pathlib.Path(path)
    if not src.exists():
        console.print(f"[red]Archivo no encontrado: {path}[/red]")
        return
    if src.suffix != ".orx":
        console.print(f"[red]Se esperaba un archivo .orx, no {src.suffix}[/red]")
        return

    import time
    source = src.read_text(encoding="utf-8")
    t0 = time.perf_counter()
    errors = []
    try:
        tokens = lex(source)
        ast = parse(tokens)
        elapsed = (time.perf_counter() - t0) * 1000
        console.print(
            f"[green]✔[/green] Sintaxis válida: [cyan]{src}[/cyan]  "
            f"[dim]({len(ast)} nodos, {elapsed:.1f} ms)[/dim]"
        )
    except Exception as e:
        elapsed = (time.perf_counter() - t0) * 1000
        console.print(f"[red]✖[/red] [bold red]Error de sintaxis[/bold red] en [cyan]{src}[/cyan]")
        console.print(f"   [red]{e}[/red]")


def cmd_add(pkg_name: str, force: bool = False):
    """orion add <pkg>"""
    from orion.pkg import add as _add
    msg = _add(pkg_name, force=force)
    color = "green" if msg.startswith("[ok]") else "yellow" if "ya instalado" in msg else "red"
    console.print(f"[{color}]{msg}[/{color}]")


def cmd_remove(pkg_name: str):
    """orion remove <pkg>"""
    from orion.pkg import remove as _remove
    msg = _remove(pkg_name)
    color = "green" if msg.startswith("[ok]") else "red"
    console.print(f"[{color}]{msg}[/{color}]")


def cmd_list():
    """orion list"""
    from orion.pkg import list_installed
    pkgs = list_installed()
    if not pkgs:
        console.print("[yellow]No hay paquetes instalados. Usa 'orion add <paquete>'[/yellow]")
        return

    t = Table(
        title="[bold cyan]Paquetes instalados[/bold cyan]",
        show_header=True,
        header_style="bold cyan",
        border_style="blue",
    )
    t.add_column("Paquete",     style="cyan",   width=16)
    t.add_column("Versión",     style="white",  width=10)
    t.add_column("Fuente",      style="dim",    width=10)
    t.add_column("Descripción", style="white")
    for p in pkgs:
        t.add_row(p["name"], p.get("version", "?"), p.get("source", "?"), p.get("description", ""))
    console.print(t)


def cmd_search(query: str):
    """orion search <query>"""
    from orion.pkg import search as _search
    results = _search(query)
    if not results:
        console.print(f"[yellow]Sin resultados para '{query}'[/yellow]")
        return

    t = Table(
        title=f"[bold cyan]Resultados para '{query}'[/bold cyan]",
        show_header=True,
        header_style="bold cyan",
        border_style="blue",
    )
    t.add_column("Paquete",     style="cyan",   width=14)
    t.add_column("Versión",     style="white",  width=8)
    t.add_column("Tipo",        style="dim",    width=10)
    t.add_column("Tags",        style="green",  width=22)
    t.add_column("Descripción", style="white")
    for r in results:
        tags = ", ".join(r.get("tags", []))
        t.add_row(r["name"], r.get("version", "?"), r.get("type", "?"), tags, r.get("description", ""))
    console.print(t)
    console.print(f"\n[dim]Instala con:[/dim] [cyan]orion add <paquete>[/cyan]")


def cmd_update(pkg_name: str | None = None):
    """orion update [pkg]"""
    from orion.pkg import update as _update
    messages = _update(pkg_name)
    for msg in messages:
        color = "green" if msg.startswith("[ok]") else "yellow" if "info" in msg else "red"
        console.print(f"[{color}]{msg}[/{color}]")


# ╔══════════════════════════════════════════════════════╗
# ║             SECCIÓN: MODO VISUAL CON TEXTUAL         ║
# ╚══════════════════════════════════════════════════════╝
try:
    from textual.app import App, ComposeResult
    from textual.widgets import Header, Footer, Static, Input, ProgressBar
    from textual.reactive import reactive
    from textual.containers import Container
    import asyncio

    class OrionVisual(App):
        """Interfaz visual tipo Gemini con animaciones"""
        CSS_PATH = None
        progress = reactive(0)

        async def on_mount(self):
            self.logo = Static(Text("> ORION", style="bold cyan", justify="center"))
            self.status = Static("[italic cyan]Cognitive Runtime Environment[/italic cyan]")
            self.progress_bar = ProgressBar(total=100)
            self.input = Input(placeholder="> Escribe un comando Orion...")
            
            # Inicializar variables y funciones para mantener estado
            self.variables = {}
            self.functions = {}

            layout = Container(self.logo, self.status, self.progress_bar, self.input)
            await self.view.dock(Header(), edge="top")
            await self.view.dock(Footer(), edge="bottom")
            await self.view.dock(layout, edge="top")

            self.set_interval(0.05, self.update_progress)

        def update_progress(self):
            if self.progress < 100:
                self.progress += 2
                self.progress_bar.progress = self.progress
                shade = int((self.progress / 100) * 255)
                color = f"#{shade:02x}{255 - shade:02x}ff"
                self.logo.update(Text("> ORION", style=f"bold {color}", justify="center"))
            else:
                self.status.update("[green]Listo para ejecutar comandos Orion[/green]")

        async def on_input_submitted(self, event: Input.Submitted):
            cmd = event.value.strip()
            if not cmd:
                return
            
            try:
                self.status.update(f"[cyan]Ejecutando:[/cyan] {cmd}")
                tokens = lex(cmd)
                ast = parse(tokens)
                # Usar las variables y funciones de la instancia
                result = evaluate(ast, self.variables, self.functions)
                if result is not None:
                    self.status.update(f"[green]✔ Resultado: {result}[/green]")
                else:
                    self.status.update(f"[green]✔ Comando '{cmd}' completado[/green]")
            except Exception as e:
                self.status.update(f"[red]✖ Error: {e}[/red]")
            
            self.input.value = ""

    def visual_mode():
        """Inicia el modo visual animado"""
        OrionVisual().run()

except ImportError:
    def visual_mode():
        console.print("[yellow]El modo visual requiere instalar 'textual'[/yellow]")
        console.print("👉 Ejecuta: pip install textual rich pyfiglet\n")


# ╔══════════════════════════════════════════════════════╗
# ║                    FUNCIÓN MAIN                      ║
# ╚══════════════════════════════════════════════════════╝
def main():
    # Interceptar subcomandos del gestor de paquetes antes de argparse
    _raw = sys.argv[1:]
    if _raw:
        sub = _raw[0].lower()

        if sub == "doctor":
            banner()
            cmd_doctor()
            return

        if sub == "watch" and len(_raw) >= 2:
            banner()
            runs_flag = next((int(x.split("=")[1]) for x in _raw if x.startswith("--runs=")), None)
            cmd_watch(_raw[1])
            return

        if sub == "bench" and len(_raw) >= 2:
            banner()
            runs = next((int(x.split("=")[1]) for x in _raw if x.startswith("--runs=")), 10)
            cmd_bench(_raw[1], runs=runs)
            return

        if sub == "test":
            banner()
            folder = _raw[1] if len(_raw) >= 2 and not _raw[1].startswith("--") else "."
            cmd_test(folder)
            return

        if sub == "new" and len(_raw) >= 2:
            banner()
            cmd_new(_raw[1])
            return

        if sub == "build" and len(_raw) >= 2:
            banner()
            cmd_build(_raw[1])
            return

        if sub == "check" and len(_raw) >= 2:
            banner()
            cmd_check(_raw[1])
            return

        if sub == "add" and len(_raw) >= 2:
            force = "--force" in _raw
            banner()
            cmd_add(_raw[1], force=force)
            return

        if sub == "remove" and len(_raw) >= 2:
            banner()
            cmd_remove(_raw[1])
            return

        if sub == "list":
            banner()
            cmd_list()
            return

        if sub == "search" and len(_raw) >= 2:
            banner()
            cmd_search(" ".join(_raw[1:]))
            return

        if sub == "update":
            banner()
            target = _raw[1] if len(_raw) >= 2 and not _raw[1].startswith("--") else None
            cmd_update(target)
            return

    parser = argparse.ArgumentParser(description="Orion Language CLI", add_help=False)
    parser.add_argument("file", nargs="?", help="Archivo Orion .orx o comando 'repl'")
    parser.add_argument("--version", action="store_true", help="Versión de Orion")
    parser.add_argument("--help", "-h", action="store_true", help="Ayuda de Orion")
    parser.add_argument("--visual", action="store_true", help="Inicia la interfaz visual animada")

    args = parser.parse_args()

    if args.help:
        banner()
        show_help()
        return

    if args.visual:
        visual_mode()
        return

    banner()

    if args.version:
        console.print(f"\n✨ [bold blue]Orion Language v{VERSION}[/bold blue]")
        return

    if args.file == "repl":
        repl()
    elif args.file and args.file.endswith(".orx"):
        run_script(args.file)
    else:
        console.print("\n💡 [yellow]Uso recomendado:[/yellow]")
        console.print("   orion <archivo.orx>       - Ejecutar un programa")
        console.print("   orion repl                - Modo interactivo")
        console.print("   orion new <proyecto>      - Crear proyecto backend")
        console.print("   orion build <archivo>     - Compilar sin ejecutar")
        console.print("   orion check <archivo>     - Verificar sintaxis")
        console.print("   orion watch <archivo>     - Hot reload al guardar")
        console.print("   orion bench <archivo>     - Benchmark de ejecución")
        console.print("   orion test [carpeta]      - Ejecutar todos los tests")
        console.print("   orion doctor              - Verificar entorno")
        console.print("   orion add <paquete>       - Instalar un paquete")
        console.print("   orion list                - Paquetes instalados")
        console.print("   orion search <query>      - Buscar paquetes")
        console.print("   orion --help              - Ayuda completa\n")

        if args.file:
            console.print(f"[red]Archivo no válido o no encontrado: {args.file}[/red]")
            sys.exit(1)

if __name__ == "__main__":
    main()