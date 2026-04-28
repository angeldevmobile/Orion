use std::io::{self, Write};
use std::thread;
use std::time::Duration;

//    Paleta moderna: azul eléctrico + cyan + naranja                           
pub const RESET:  &str = "\x1b[0m";
pub const BOLD:   &str = "\x1b[1m";
pub const DIM:    &str = "\x1b[2m";
pub const RED:    &str = "\x1b[31m";
pub const GREEN:  &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const CYAN:   &str = "\x1b[36m";
pub const WHITE:  &str = "\x1b[37m";
pub const BBLUE:  &str = "\x1b[94m";   // azul eléctrico — color principal
pub const BCYAN:  &str = "\x1b[96m";   // cyan brillante — acento
pub const BWHITE: &str = "\x1b[97m";   // blanco brillante
pub const ORANGE: &str = "\x1b[38;5;208m"; // naranja — acento secundario
#[allow(dead_code)] pub const BMAGENTA: &str = "\x1b[95m";
#[allow(dead_code)] pub const BGREEN:   &str = "\x1b[92m";
#[allow(dead_code)] pub const BYELLOW:  &str = "\x1b[93m";

// Alias usados por otros módulos
pub use self::RED   as RED_MOD;
pub use self::GREEN as GREEN_MOD;

//    Animación de inicio                                                        

pub fn animate_startup() {
    let stdout = io::stdout();

    // Fase 1 — spinner braille (moderno, rápido)
    let frames = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
    let msg = "  Initializing Orion Runtime";
    for i in 0..18 {
        let f = frames[i % frames.len()];
        let dots = ".".repeat((i % 4) + 1);
        {
            let mut out = stdout.lock();
            write!(out, "\r  {BBLUE}{BOLD}{f}{RESET}  {DIM}{WHITE}{msg}{dots:<4}{RESET}   ").ok();
            out.flush().ok();
        }
        thread::sleep(Duration::from_millis(55));
    }

    // Fase 2 — barra de progreso con estilo
    let total = 28usize;
    for i in 0..=total {
        let filled = i;
        let empty  = total - i;
        let bar: String = format!(
            "{BBLUE}{BOLD}{}{RESET}{DIM}{}{RESET}",
            "█".repeat(filled),
            "░".repeat(empty)
        );
        let pct = (i * 100) / total;
        {
            let mut out = stdout.lock();
            write!(out, "\r  {DIM}[{RESET}{bar}{DIM}]{RESET}  {ORANGE}{BOLD}{pct:>3}%{RESET}   ").ok();
            out.flush().ok();
        }
        thread::sleep(Duration::from_millis(28));
    }

    // Limpiar línea
    {
        let mut out = stdout.lock();
        write!(out, "\r{:<70}\r", "").ok();
        out.flush().ok();
    }
    thread::sleep(Duration::from_millis(80));
}

//    Banner principal                                                           

pub fn print_banner() {
    println!();
    // ASCII art — gradiente azul eléctrico = cyan
    println!("  {BOLD}{BBLUE}  ██████╗ ██████╗ ██╗ ██████╗ ███╗   ██╗{RESET}");
    println!("  {BOLD}{BBLUE} ██╔═══██╗██╔══██╗██║██╔═══██╗████╗  ██║{RESET}");
    println!("  {BOLD}{BCYAN} ██║   ██║██████╔╝██║██║   ██║██╔██╗ ██║{RESET}");
    println!("  {BOLD}{BCYAN} ██║   ██║██╔══██╗██║██║   ██║██║╚██╗██║{RESET}");
    println!("  {BOLD}{BBLUE} ╚██████╔╝██║  ██║██║╚██████╔╝██║ ╚████║{RESET}");
    println!("  {BOLD}{BBLUE}  ╚═════╝ ╚═╝  ╚═╝╚═╝ ╚═════╝ ╚═╝  ╚═══╝{RESET}");
    println!();
    // Línea de acento naranja
    println!("  {ORANGE}{BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}");
    println!("  {BWHITE}{BOLD}  Language Runtime{RESET}  {DIM}v0.4.0{RESET}  \
              {DIM}·{RESET}  {BCYAN}Fast · Safe · Expressive{RESET}");
    println!("  {ORANGE}{BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}");
    println!();
}

//    Helpers de output                                                          

pub fn ok(msg: &str)   { println!("  {BOLD}{GREEN}✓{RESET}  {msg}"); }
pub fn info(msg: &str) { println!("  {BOLD}{BCYAN}i{RESET}  {msg}"); }
pub fn warn(msg: &str) { println!("  {BOLD}{ORANGE}!{RESET}  {msg}"); }
pub fn fail(msg: &str) { eprintln!("  {BOLD}{RED}✗{RESET}  {msg}"); }

pub fn section(title: &str) {
    println!("\n  {BOLD}{BCYAN}{title}{RESET}");
    println!("  {DIM}{BBLUE}{}{RESET}", " ".repeat(title.chars().count() + 2));
}

pub fn row(label: &str, value: &str, good: bool) {
    let icon = if good {
        format!("{BOLD}{GREEN}✓{RESET}")
    } else {
        format!("{BOLD}{RED}✗{RESET}")
    };
    println!("  {icon}  {DIM}{label:<26}{RESET}{BWHITE}{value}{RESET}");
}

pub fn table_header(cols: &[&str]) {
    let row: String = cols.iter()
        .map(|c| format!("{BOLD}{BCYAN}{c:<20}{RESET}"))
        .collect();
    println!("  {row}");
    println!("  {DIM}{BBLUE}{}{RESET}", " ".repeat(cols.len() * 20));
}

pub fn table_row(cols: &[&str]) {
    let row: String = cols.iter()
        .map(|c| format!("{BWHITE}{c:<20}{RESET}"))
        .collect();
    println!("  {row}");
}
