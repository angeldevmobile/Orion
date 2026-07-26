#!/usr/bin/env node
// Genera src/cli/builtins_gen.rs a partir de los match-arms de src/modules/*.rs.
//
// Cada módulo de la stdlib es un dispatcher `pub fn call(function, args)` con
// arms `"nombre" => { ... }` y (por convención) un comentario encima con la
// forma `// nombre(args) → descripción`. Este script extrae ese contrato y lo
// vuelca al registro de builtins (la typeshed que consume el LSP vía
// `orion --builtins-json`), para que hover/autocompletado cubran TODOS los
// módulos sin documentarlos dos veces.
//
// Uso:  node scripts/gen_builtins.js
// Los módulos curados a mano en builtins.rs (gui, state, json, …) ganan:
// registry() deduplica por `qualified` dando prioridad a lo curado.

const fs = require('fs');
const path = require('path');

const SRC = path.join(__dirname, '..', 'src', 'modules');
const OUT = path.join(__dirname, '..', 'src', 'cli', 'builtins_gen.rs');

// 1. Mapa módulo→archivo desde el dispatcher de mod.rs: `"fs" => fs::call`
const modRs = fs.readFileSync(path.join(SRC, 'mod.rs'), 'utf8');
const moduleMap = []; // { names: ["table","df"], file: "table_mod.rs" }
for (const m of modRs.matchAll(/^\s*((?:"[a-z_0-9]+"\s*\|\s*)*"[a-z_0-9]+")\s*=>\s*([a-z_0-9]+)::call/gm) ) {
    const names = [...m[1].matchAll(/"([a-z_0-9]+)"/g)].map(x => x[1]);
    moduleMap.push({ names, file: m[2] + '.rs' });
}

// 2. Extraer arms del `pub fn call` de cada módulo, con su comentario.
function extractFns(file) {
    let src;
    try { src = fs.readFileSync(path.join(SRC, file), 'utf8'); }
    catch {
        // Módulos-directorio (gui, tui): "gui" => gui::call vive en gui/mod.rs
        try { src = fs.readFileSync(path.join(SRC, file.replace(/\.rs$/, ''), 'mod.rs'), 'utf8'); }
        catch { return []; }
    }
    const lines = src.split(/\r?\n/);

    // Localizar `pub fn call` y recorrer con profundidad de llaves para
    // quedarnos solo con los arms del match de primer nivel (depth 2:
    // cuerpo del fn = 1, cuerpo del match = 2). Los match anidados quedan fuera.
    const start = lines.findIndex(l => /pub fn call\s*\(/.test(l));
    if (start < 0) return [];

    const fns = [];
    let depth = 0, started = false, comments = [];
    for (let i = start; i < lines.length; i++) {
        const line = lines[i];
        const code = line.replace(/"(?:[^"\\]|\\.)*"/g, '""').replace(/\/\/.*$/, '');
        const opens = (code.match(/{/g) || []).length;
        const closes = (code.match(/}/g) || []).length;
        const depthBefore = depth;
        depth += opens - closes;
        if (!started) { if (opens > 0) started = true; comments = []; continue; }
        if (started && depth <= 0) break; // fin de pub fn call

        const trimmed = line.trim();
        if (trimmed.startsWith('//')) { comments.push(trimmed.replace(/^\/\/\s?/, '')); continue; }

        if (depthBefore === 2) {
            // Los nombres llevan mayúsculas ("rot2D", "gate_CNOT") y alias en
            // español ("tamaño"). Con un patrón solo-minúsculas-ASCII, un único
            // carácter fuera de rango descartaba el arm ENTERO y se llevaba por
            // delante a sus hermanos: `"len" | "size" | "tamaño"` perdía los tres.
            // Esas ausencias hacen que el typechecker marque como inexistentes
            // funciones reales y aborte programas que funcionan.
            const arm = trimmed.match(/^((?:"[\p{L}\p{N}_]+"\s*\|\s*)*"[\p{L}\p{N}_]+")\s*=>/u);
            if (arm) {
                const names = [...arm[1].matchAll(/"([\p{L}\p{N}_]+)"/gu)].map(x => x[1]);
                fns.push({ names, comment: comments.join(' ').trim() });
            }
        }
        if (trimmed !== '') comments = [];
    }
    return fns;
}

// 3. Comentario `nombre(args) → desc` → firma y descripción.
function parseDoc(mod, name, comment) {
    let signature = `${mod}.${name}(…)`;
    let description = `Función del módulo ${mod}.`;
    // Fuera separadores de sección tipo "--- Archivos ---"
    comment = (comment || '').replace(/-{2,}[^-]*-{2,}/g, '').trim();
    // Si el contrato aparece más adentro (títulos de sección acumulados antes,
    // o convención "gui.progress(...)" con módulo incluido), cortar hasta él.
    const at = Math.max(comment.indexOf(name + '('), comment.indexOf(mod + '.' + name + '('));
    if (at > 0) comment = comment.slice(at).replace(new RegExp('^' + mod + '\\.'), '');
    if (comment.startsWith(mod + '.')) comment = comment.slice(mod.length + 1);
    if (comment) {
        const arrow = comment.split(/\s*→\s*/);
        const head = arrow[0].trim();
        const tail = arrow.slice(1).join(' → ').trim();
        if (head.startsWith(name + '(') || head === name) {
            // Lo que sigue a → es descripción, no tipo de retorno: no va en la firma.
            signature = `${mod}.${head}`;
            if (tail) description = cap(tail);
        } else {
            description = cap(comment);
        }
    }
    return { signature, description };
}
const cap = s => s ? s[0].toUpperCase() + s.slice(1) : s;
const esc = s => s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');

// 4. Emitir Rust.
let out = `//! GENERADO por scripts/gen_builtins.js — NO editar a mano.
//! Regenerar con: node scripts/gen_builtins.js
//! Extrae los match-arms de src/modules/*.rs y sus comentarios de contrato.

use super::builtins::BuiltinDoc;

fn f(module: &str, name: &str, signature: &str, description: &str) -> BuiltinDoc {
    BuiltinDoc {
        name: name.into(),
        qualified: format!("{module}.{name}"),
        owner: module.into(),
        kind: "module".into(),
        signature: signature.into(),
        description: description.into(),
        example: String::new(),
    }
}

pub fn generated_modules(v: &mut Vec<BuiltinDoc>) {
`;

let total = 0, perMod = [];
for (const { names, file } of moduleMap) {
    const fns = extractFns(file);
    if (fns.length === 0) continue;
    const mod = names[0]; // nombre canónico; alias de módulo comparten entradas
    out += `    // ${mod} (${file})\n`;
    for (const fn of fns) {
        const primary = fn.names[0];
        const { signature, description } = parseDoc(mod, primary, fn.comment);
        out += `    v.push(f("${mod}", "${primary}", "${esc(signature)}", "${esc(description)}"));\n`;
        for (const alias of fn.names.slice(1)) {
            out += `    v.push(f("${mod}", "${alias}", "${esc(signature.replace(mod + '.' + primary, mod + '.' + alias))}", "Alias de ${mod}.${primary}."));\n`;
        }
        total += fn.names.length;
    }
    perMod.push(`${mod}:${fns.length}`);
}
out += `}\n`;

// Escritura idempotente: si el contenido no cambió, no tocar el mtime
// (build.rs corre esto en cada build; reescribir igual forzaría recompilar).
let prev = '';
try { prev = fs.readFileSync(OUT, 'utf8'); } catch {}
if (prev === out) {
    console.log(`OK (sin cambios) → ${OUT}`);
} else {
    fs.writeFileSync(OUT, out, 'utf8');
    console.log(`OK → ${OUT}`);
}
console.log(`${total} funciones en ${perMod.length} módulos`);
console.log(perMod.join('  '));
