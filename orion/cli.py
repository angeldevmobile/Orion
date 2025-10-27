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
    """Muestra el banner tipo Gemini con gradiente"""
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
    """Ejecuta un archivo Orion .orx"""
    if not os.path.exists(path):
        console.print(f"[red]Archivo no encontrado:[/red] {path}")
        sys.exit(1)

    console.print(f"[blue]Ejecutando:[/blue] {path}\n")
    with open(path, "r", encoding="utf-8") as f:
        source = f.read()
    
    try:
        tokens = lex(source)
        ast = parse(tokens)
        # Agregar variables y funciones
        variables = {}
        functions = {}
        result = evaluate(ast, variables, functions)
        if result is not None:
            console.print(Panel(str(result), title="Resultado", border_style="green"))
        console.print("\n[green]Ejecución completada[/green]")
    except Exception as e:
        console.print(f"\n[bold red]Error durante la ejecución:[/bold red] {e}")
        sys.exit(1)


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

    help_table.add_column("Comando", style="cyan", width=25)
    help_table.add_column("Descripción", style="white")
    help_table.add_column("Ejemplo", style="green")

    help_table.add_row("orion <archivo.orx>", "Ejecuta un archivo Orion", "orion programa.orx")
    help_table.add_row("orion repl", "Inicia el entorno interactivo REPL", "orion repl")
    help_table.add_row("--visual", "Inicia el modo visual animado", "orion --visual")
    help_table.add_row("--version", "Muestra la versión de Orion", "orion --version")
    help_table.add_row("--help", "Muestra esta ayuda", "orion --help")

    console.print(help_table)


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
        console.print("   orion <archivo.orx>  - Ejecutar un programa")
        console.print("   orion repl           - Modo interactivo")
        console.print("   orion --visual       - Modo visual animado")
        console.print("   orion --help         - Mostrar ayuda completa\n")

        if args.file:
            console.print(f"[red]Archivo no válido o no encontrado: {args.file}[/red]")
            sys.exit(1)

if __name__ == "__main__":
    main()