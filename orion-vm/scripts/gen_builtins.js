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

// Tipos de retorno que aparecen tras la flecha en los comentarios de contrato.
//
// La capitalización es libre en el código (`Bool`, `bool`, `List`, `list`), así
// que se compara en minúsculas y se normaliza al nombre que usa el lenguaje.
// Los sinónimos en español están porque los comentarios los mezclan.
const TIPOS_RETORNO = {
    bool: 'bool', booleano: 'bool',
    int: 'int', entero: 'int',
    float: 'float', flotante: 'float',
    string: 'string', str: 'string', texto: 'string', cadena: 'string',
    list: 'list', lista: 'list',
    dict: 'dict', diccionario: 'dict',
    handle: 'handle',
    nada: 'nada', none: 'nada', null: 'nada', void: 'nada',
};

// Separa un tipo de retorno de la prosa que lo acompaña.
//
// El convenio de los módulos es `nombre(args) → tipo resto de la explicación`,
// y hasta ahora TODO lo que seguía a la flecha se guardaba como descripción: el
// tipo estaba escrito en ~190 funciones y se tiraba, dejando el hover sin un
// solo tipo de retorno. Solo se mira aquí, después de la flecha, porque en esa
// posición una palabra como "lista" es el tipo; al principio de una frase suele
// ser el verbo ("Lista el contenido de una carpeta") y confundirlos daría
// firmas mentira, que es peor que no tener firma.
function partirRetorno(tail) {
    const m = tail.match(/^([\p{L}]+)\b(.*)$/u);
    if (!m) return { ret: null, resto: tail };
    const tipo = TIPOS_RETORNO[m[1].toLowerCase()];
    if (!tipo) return { ret: null, resto: tail };
    // Lo que sigue al tipo se limpia de separadores sueltos (`—`, `:`, `,`).
    const resto = m[2].replace(/^\s*[—\-:,]\s*/, '').trim();
    return { ret: tipo, resto };
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
        // El separador de contrato es `→` (479 comentarios). Tres usan raya
        // larga, y sin contemplarla la descripción entera acababa DENTRO de la
        // firma: `gui.section(titulo, accion?) … — card con cabecera y acción
        // opcional. Deja la card ABIERTA…` se mostraba como si todo eso fuera
        // la lista de parámetros. Se admite como alternativa solo cuando no hay
        // flecha, para no partir una raya que sea parte de la explicación.
        const sep = comment.includes('→') ? /\s*→\s*/ : /\s*…?\s*—\s*/;
        const arrow = comment.split(sep);
        const head = arrow[0].trim();
        const tail = arrow.slice(1).join(' — ').trim();
        if (head.startsWith(name + '(') || head === name) {
            signature = `${mod}.${head}`;
            if (tail) {
                // Si tras la flecha viene un tipo, va a la FIRMA (`-> dict`) y
                // solo el resto queda como descripción. Así el hover dice qué
                // devuelve, que es lo que hace falta para encadenar llamadas.
                // Si el contrato ya declara el retorno en la cabecera
                // (`upper(s: string) -> string → en mayúsculas`), no se vuelve a
                // añadir: sin esta guarda salía `-> string -> string` en cuanto
                // la descripción empezaba por una palabra que parece un tipo.
                const { ret, resto } = head.includes('->')
                    ? { ret: null, resto: tail }
                    : partirRetorno(tail);
                if (ret) {
                    signature = `${mod}.${head} -> ${ret}`;
                    // Un comentario que era solo el tipo (`→ bool`) no deja
                    // prosa: mejor decir qué devuelve que repetir "Función del
                    // módulo X", que no informa de nada.
                    description = resto ? cap(resto) : `Devuelve ${ret}.`;
                } else {
                    description = cap(tail);
                }
            }
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
        // Los rellena builtins::registry() desmenuzando la firma; aquí no se
        // duplica el parseo para que no haya dos versiones que puedan discrepar.
        params: Vec::new(),
        returns: None,
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
