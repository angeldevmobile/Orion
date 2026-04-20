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


def repl():
    """Modo interactivo Orion REPL"""
    console.print(
        Panel(
            "[bold]Orion REPL — Escribe código Orion interactivamente[/bold]\n"
            "[italic]Presiona Ctrl+C o escribe 'exit' para salir[/italic]",
            style="bold blue",
            border_style="bright_blue",
        )
    )

    variables = {}
    functions = {}
    line_count = 0
    
    while True:
        try:
            line_count += 1
            src = Prompt.ask(f"[bold cyan]> orion:{line_count:02d}[/bold cyan]")

            if src.strip().lower() in ['exit', 'quit', 'salir']:
                break
            if not src.strip():
                continue

            tokens = lex(src)
            ast = parse(tokens)
            result = evaluate(ast, variables, functions)
            if result is not None:
                console.print(f"➤ [green]{result}[/green]")

        except (KeyboardInterrupt, EOFError):
            console.print("\n👋 [yellow]Saliendo del REPL.[/yellow]")
            break
        except Exception as e:
            console.print(f"[red]{e}[/red]")


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
# ║              SECCIÓN: PACKAGE MANAGER                ║
# ╚══════════════════════════════════════════════════════╝
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
        console.print("   orion add <paquete>       - Instalar un paquete")
        console.print("   orion list                - Paquetes instalados")
        console.print("   orion search <query>      - Buscar paquetes")
        console.print("   orion --help              - Ayuda completa\n")

        if args.file:
            console.print(f"[red]Archivo no válido o no encontrado: {args.file}[/red]")
            sys.exit(1)

if __name__ == "__main__":
    main()