//! Core del debugger de Orion.
//!
//! `DebugSession` envuelve la VM y controla la ejecución instrucción a instrucción,
//! gestionando breakpoints, modos de step y watches.

use crate::vm::VM;
use crate::bytecode::OrionBytecode;
use crate::value::Value;

//     Tipos públicos                                                            

/// Un punto de pausa definido por número de línea.
#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub id:        usize,
    pub line:      u32,
    /// Condición textual (evaluación futura; hoy se ignora si es `Some`).
    pub condition: Option<String>,
    pub enabled:   bool,
    pub hit_count: u32,
}

/// Modo de ejecución paso a paso.
#[derive(Debug, Clone, PartialEq)]
pub enum StepMode {
    /// Ejecutar libremente hasta el próximo breakpoint.
    Continue,
    /// Pausar en cuanto cambie la línea o la profundidad del call stack.
    StepInto,
    /// Pausar cuando la profundidad vuelva a ser ≤ `target_depth` en una línea distinta.
    StepOver { target_depth: usize, start_line: u32 },
    /// Pausar cuando la profundidad baje por debajo de `target_depth`.
    StepOut  { target_depth: usize },
}

/// Motivo por el que la sesión está pausada.
#[derive(Debug, Clone, PartialEq)]
pub enum PauseReason {
    /// Pausa inicial al arrancar el programa.
    Entry,
    /// Pausa por completar un step.
    Step,
    /// Pausa por breakpoint.
    Breakpoint { id: usize, line: u32 },
    /// Error de runtime no capturado.
    Error(String),
    /// Pausa solicitada explícitamente (futuro: Ctrl-C en DAP).
    UserPause,
}

/// Información de un frame para el DAP / backtrace.
#[derive(Debug, Clone)]
pub struct DebugFrame {
    pub id:   usize,
    pub name: String,
    pub line: u32,
}

//     DebugSession                                                              

pub struct DebugSession {
    pub vm:           VM,
    breakpoints:      Vec<Breakpoint>,
    next_bp_id:       usize,
    pub step_mode:    StepMode,
    pub paused:       bool,
    pub pause_reason: Option<PauseReason>,
    pub source_lines: Vec<String>,
    pub watches:      Vec<String>,
    pub done:         bool,
}

impl DebugSession {
    /// Crea una sesión nueva. El programa arranca pausado en la primera línea.
    pub fn new(bc: OrionBytecode, source: &str) -> Self {
        let vm = VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
        DebugSession {
            vm,
            breakpoints:  Vec::new(),
            next_bp_id:   1,
            step_mode:    StepMode::Continue,
            paused:       true,
            pause_reason: Some(PauseReason::Entry),
            source_lines: source.lines().map(String::from).collect(),
            watches:      Vec::new(),
            done:         false,
        }
    }

    //     Gestión de breakpoints                                                

    pub fn add_breakpoint(&mut self, line: u32, condition: Option<String>) -> usize {
        let id = self.next_bp_id;
        self.next_bp_id += 1;
        self.breakpoints.push(Breakpoint {
            id, line, condition, enabled: true, hit_count: 0,
        });
        id
    }

    /// Elimina el breakpoint con ese id **o** esa línea.
    pub fn remove_breakpoint(&mut self, n: u32) {
        self.breakpoints.retain(|bp| bp.id as u32 != n && bp.line != n);
    }

    /// Alterna habilitado/deshabilitado. Devuelve el nuevo estado.
    pub fn toggle_breakpoint(&mut self, n: u32) -> Option<bool> {
        for bp in &mut self.breakpoints {
            if bp.id as u32 == n || bp.line == n {
                bp.enabled = !bp.enabled;
                return Some(bp.enabled);
            }
        }
        None
    }

    pub fn list_breakpoints(&self) -> &[Breakpoint] {
        &self.breakpoints
    }

    /// Actualiza los breakpoints de un archivo (usado por el DAP al recibir `setBreakpoints`).
    pub fn set_breakpoints_for_file(&mut self, lines: &[u32]) -> Vec<usize> {
        self.breakpoints.clear();
        lines.iter().map(|&l| self.add_breakpoint(l, None)).collect()
    }

    fn check_bp(&mut self, line: u32) -> Option<usize> {
        for bp in &mut self.breakpoints {
            if bp.enabled && bp.line == line {
                bp.hit_count += 1;
                return Some(bp.id);
            }
        }
        None
    }

    //     Ejecución controlada                                                 

    /// Ejecuta el programa hasta el próximo punto de pausa:
    /// breakpoint, step completado, error, o fin del programa.
    pub fn run_until_pause(&mut self) -> Result<(), String> {
        loop {
            if self.done { break; }

            let line_before  = self.vm.current_line();
            let depth_before = self.vm.call_depth();

            match self.vm.step_once() {
                Ok(true) => {
                    self.done = true;
                    self.paused = true;
                    break;
                }
                Ok(false) => {
                    let line_now  = self.vm.current_line();
                    let depth_now = self.vm.call_depth();
                    let line_changed = line_now != line_before && line_now != 0;

                    match self.step_mode.clone() {
                        StepMode::StepInto => {
                            if line_changed || depth_now != depth_before {
                                self.paused = true;
                                self.pause_reason = Some(PauseReason::Step);
                                break;
                            }
                        }
                        StepMode::StepOver { target_depth, start_line } => {
                            if depth_now <= target_depth && line_now != start_line && line_now != 0 {
                                self.step_mode = StepMode::Continue;
                                self.paused = true;
                                self.pause_reason = Some(PauseReason::Step);
                                break;
                            }
                        }
                        StepMode::StepOut { target_depth } => {
                            if depth_now < target_depth {
                                self.step_mode = StepMode::Continue;
                                self.paused = true;
                                self.pause_reason = Some(PauseReason::Step);
                                break;
                            }
                        }
                        StepMode::Continue => {}
                    }

                    // Revisar breakpoints solo cuando la línea cambia
                    if line_changed {
                        if let Some(bp_id) = self.check_bp(line_now) {
                            self.step_mode = StepMode::Continue;
                            self.paused = true;
                            self.pause_reason = Some(PauseReason::Breakpoint {
                                id: bp_id, line: line_now,
                            });
                            break;
                        }
                    }
                }
                Err(e) => {
                    self.done = true;
                    self.paused = true;
                    self.pause_reason = Some(PauseReason::Error(e.clone()));
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    //     Comandos de control                                                  

    pub fn do_continue(&mut self) {
        self.step_mode = StepMode::Continue;
        self.paused = false;
    }

    pub fn do_step_into(&mut self) {
        self.step_mode = StepMode::StepInto;
        self.paused = false;
    }

    pub fn do_step_over(&mut self) {
        let depth = self.vm.call_depth();
        let line  = self.vm.current_line();
        self.step_mode = StepMode::StepOver { target_depth: depth, start_line: line };
        self.paused = false;
    }

    pub fn do_step_out(&mut self) {
        let depth = self.vm.call_depth();
        self.step_mode = StepMode::StepOut { target_depth: depth };
        self.paused = false;
    }

    //     Introspección                                                        

    /// Líneas de contexto alrededor de `line` (±`radius`).
    /// Devuelve `(número_línea, texto, es_la_línea_actual)`.
    pub fn source_context(&self, line: u32, radius: u32) -> Vec<(u32, &str, bool)> {
        if line == 0 { return Vec::new(); }
        let start = line.saturating_sub(radius).max(1);
        let end   = (line + radius).min(self.source_lines.len() as u32);
        (start..=end).filter_map(|l| {
            self.source_lines
                .get((l - 1) as usize)
                .map(|s| (l, s.as_str(), l == line))
        }).collect()
    }

    /// Frames del call stack para el debugger (más reciente primero).
    pub fn debug_frames(&self) -> Vec<DebugFrame> {
        self.vm.debug_frames().into_iter().enumerate()
            .map(|(i, (name, line))| DebugFrame { id: i, name, line })
            .collect()
    }

    /// Variables del frame con índice `frame_id` (0 = más reciente).
    pub fn frame_vars(&self, frame_id: usize) -> Vec<(String, Value)> {
        let all = self.vm.debug_all_scopes();
        all.into_iter().nth(frame_id).unwrap_or_default()
    }

    /// Busca una variable en la cadena de scopes.
    pub fn lookup_var(&self, name: &str) -> Option<Value> {
        self.vm.debug_lookup_var(name)
    }

    //     Watches                                                              

    pub fn add_watch(&mut self, expr: String) -> bool {
        if !self.watches.contains(&expr) {
            self.watches.push(expr);
            true
        } else {
            false
        }
    }

    pub fn remove_watch(&mut self, expr: &str) {
        self.watches.retain(|w| w != expr);
    }

    /// Evalúa todos los watches contra el scope actual.
    pub fn eval_watches(&self) -> Vec<(String, Option<Value>)> {
        self.watches.iter()
            .map(|expr| (expr.clone(), self.lookup_var(expr)))
            .collect()
    }
}
