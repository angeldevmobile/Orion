// ANSI escape codes — no external dependencies
pub const RESET:   &str = "\x1b[0m";
pub const BOLD:    &str = "\x1b[1m";
pub const DIM:     &str = "\x1b[2m";
pub const RED:     &str = "\x1b[31m";
pub const GREEN:   &str = "\x1b[32m";
pub const YELLOW:  &str = "\x1b[33m";
pub const CYAN:    &str = "\x1b[36m";
pub const WHITE:   &str = "\x1b[37m";
pub const BMAGENTA:&str = "\x1b[95m";
pub const BCYAN:   &str = "\x1b[96m";
#[allow(dead_code)] pub const BGREEN:  &str = "\x1b[92m";
#[allow(dead_code)] pub const BYELLOW: &str = "\x1b[93m";

pub fn print_banner() {
    println!("{BOLD}{CYAN}");
    println!("  ╔══════════════════════════════════════╗");
    println!("  ║  {BMAGENTA}◆ ORION{CYAN}  Language Runtime  {DIM}v0.4.0{RESET}{BOLD}{CYAN}  ║");
    println!("  ╚══════════════════════════════════════╝{RESET}");
    println!("  {DIM}{WHITE}Fast · Safe · Expressive{RESET}");
    println!();
}

pub fn ok(msg: &str)   { println!("  {BOLD}{GREEN}✓{RESET}  {msg}"); }
pub fn info(msg: &str) { println!("  {BOLD}{CYAN}i{RESET}  {msg}"); }
pub fn warn(msg: &str) { println!("  {BOLD}{YELLOW}!{RESET}  {msg}"); }
pub fn fail(msg: &str) { eprintln!("  {BOLD}{RED}✗{RESET}  {msg}"); }

pub fn section(title: &str) {
    println!("\n  {BOLD}{BCYAN}{title}{RESET}");
    println!("  {DIM}{}{RESET}", "─".repeat(title.chars().count() + 2));
}

pub fn row(label: &str, value: &str, good: bool) {
    let icon = if good {
        format!("{BOLD}{GREEN}✓{RESET}")
    } else {
        format!("{BOLD}{RED}✗{RESET}")
    };
    println!("  {icon}  {DIM}{label:<26}{RESET}{value}");
}

pub fn table_header(cols: &[&str]) {
    let row: String = cols.iter().map(|c| format!("{BOLD}{c:<20}{RESET}")).collect();
    println!("  {row}");
    println!("  {DIM}{}{RESET}", "─".repeat(cols.len() * 20));
}

pub fn table_row(cols: &[&str]) {
    let row: String = cols.iter().map(|c| format!("{c:<20}")).collect();
    println!("  {row}");
}
