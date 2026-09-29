//! Generación y edición de documentos: `pdf`, `excel.write_styled` y
//! `csv.write`.
//!
//! Regresiones que cubre:
//!   - pdf.report cortaba en silencio: 4 columnas como mucho, y al llegar al
//!     pie de la primera página dejaba de pintar filas.
//!   - Al repartir el ancho, las cifras se recortaban ("1.335.0…").
//!   - pdf.watermark borraba las fuentes de la página: la marca salía y el
//!     texto del documento dejaba de verse.
//!   - pdf.info leía como UTF-8 los metadatos guardados en UTF-16.
//!   - excel.write_styled y csv.write ordenaban las columnas alfabéticamente.
//!
//! Los programas Orion se autoverifican con `error`; lo que Orion no puede
//! mirar (las fuentes de una página) se comprueba con lopdf.

use orion_vm::{codegen, lexer, parser, vm};
use std::fs;
use std::path::PathBuf;

fn run_ok(src: &str) {
    let tokens = lexer::lex(src).unwrap_or_else(|e| panic!("lex error: {} | src:\n{}", e.message, src));
    let stmts = parser::parse(tokens).unwrap_or_else(|e| panic!("parse error: {} | src:\n{}", e.message, src));
    let bc = codegen::compile(stmts).unwrap_or_else(|e| panic!("codegen error: {} | src:\n{}", e.message, src));
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().unwrap_or_else(|e| panic!("runtime error: {} | src:\n{}", e, src));
}

fn run_err(src: &str) -> String {
    let tokens = lexer::lex(src).unwrap();
    let stmts = parser::parse(tokens).unwrap();
    let bc = codegen::compile(stmts).unwrap();
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().expect_err("se esperaba un error")
}

/// Carpeta de trabajo propia de cada test, con barras normales para poder
/// meter la ruta en un literal de Orion también en Windows.
fn carpeta(nombre: &str) -> (PathBuf, String) {
    let d = std::env::temp_dir().join("orion_tests_documents").join(nombre);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let s = d.to_string_lossy().replace('\\', "/");
    (d, s)
}

/// Un PNG con transparencia, para los bloques de imagen.
fn logo(dir: &PathBuf) {
    let mut img = image::RgbaImage::new(80, 40);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = if x < 40 { image::Rgba([31, 111, 74, 255]) } else { image::Rgba([0, 0, 0, 0]) };
    }
    img.save(dir.join("logo.png")).unwrap();
}

//   pdf.report                                ─

#[test]
fn report_pagina_y_no_pierde_filas_ni_columnas() {
    let (_d, dir) = carpeta("report");
    run_ok(&format!(r##"
        use "pdf" as pdf
        filas = [["SKU", "Producto", "Categoría", "Proveedor", "Uds", "Precio", "Importe", "Margen"]]
        i = 1
        while i <= 70 {{
            filas.push(["GEN-" + str(i), "Teclado mecánico inalámbrico con reposamuñecas " + str(i),
                        "teclados", "Nórdica Distribución", str(i * 3), "89,90 €", str(i * 269) + ",70 €", "5 %"])
            i = i + 1
        }}
        filas.push(["TOTAL", "70 productos", "", "", "7455", "", "1.335.052,50 €", ""])
        pdf.report("{dir}/r.pdf", "Ventas", filas, {{ "total": yes }})

        if pdf.pages("{dir}/r.pdf") < 2 {{ error("73 filas no caben en una página: tenía que paginar") }}
        t = pdf.read("{dir}/r.pdf")
        -- La primera y la última fila, y la octava columna: nada se perdió.
        for trozo in ["GEN-1", "GEN-70", "Margen", "1.335.052,50"] {{
            if not t.contains(trozo) {{ error("falta en el PDF: " + trozo) }}
        }}
        -- La cabecera se repite en cada página.
        if len(t.split("Proveedor")) - 1 < 2 {{ error("la cabecera no se repitió") }}
        -- Pie con número de página por defecto.
        if not t.contains("Página 1 de") {{ error("falta el número de página") }}
    "##));
}

#[test]
fn report_columnas_configurables() {
    let (_d, dir) = carpeta("columnas");
    run_ok(&format!(r##"
        use "pdf" as pdf
        filas = [
            {{ "sku": "A-1", "importe": 1234.5, "margen": 0.125, "interno": "SECRETO" }},
            {{ "sku": "A-2", "importe": 99, "margen": 0.5, "interno": "SECRETO" }}
        ]
        pdf.report("{dir}/c.pdf", "Configurado", filas, {{
            "size": "letter", "orientation": "landscape", "page_format": "{{page}}/{{pages}}",
            "header_bg": "#1f6f4a", "header_color": "white", "borders": yes,
            "columns": {{
                "sku": {{ "title": "Código", "bold": yes }},
                "importe": {{ "title": "Importe", "format": "money" }},
                "margen": {{ "format": "percent:1", "color": "red" }},
                "interno": {{ "hidden": yes }}
            }}
        }})
        t = pdf.read("{dir}/c.pdf")
        for trozo in ["Código", "1.234,50 €", "99,00 €", "12,5 %", "50,0 %", "1/1"] {{
            if not t.contains(trozo) {{ error("falta en el PDF: " + trozo) }}
        }}
        if t.contains("SECRETO") {{ error("la columna oculta salió en el PDF") }}
    "##));
}

#[test]
fn report_separadores_y_simbolo_configurables() {
    let (_d, dir) = carpeta("separadores");
    run_ok(&format!(r##"
        use "pdf" as pdf
        pdf.report("{dir}/s.pdf", "USD", [{{ "total": 1234567.891 }}], {{
            "decimal": ".", "thousands": ",",
            "columns": {{ "total": {{ "format": "money:$" }} }}
        }})
        if not pdf.read("{dir}/s.pdf").contains("1,234,567.89 $") {{ error("formato con separadores propios") }}
    "##));
}

//   pdf.build                                 

#[test]
fn build_con_todos_los_bloques() {
    let (d, dir) = carpeta("build");
    logo(&d);
    run_ok(&format!(r##"
        use "pdf" as pdf
        pdf.build("{dir}/b.pdf", [
            {{ "image": "{dir}/logo.png", "width": 30, "align": "right" }},
            {{ "title": "Factura F-1", "color": "#1f6f4a" }},
            {{ "fields": {{ "Cliente": "Íñigo Muñoz", "Dirección": "Calle Mayor 1" }} }},
            {{ "line": yes }},
            {{ "heading": "Detalle" }},
            {{ "table": [["Concepto", "Importe"], ["Teclado", 179.8]], "columns": [{{}}, {{ "format": "money" }}] }},
            {{ "text": "Total: 179,80 €", "bold": yes, "align": "right" }},
            {{ "space": 5 }},
            "Un texto suelto es un párrafo.",
            {{ "page_break": yes }},
            {{ "text": "Segunda página" }}
        ], {{ "author": "Comercio", "footer": "pie propio" }})
        if pdf.pages("{dir}/b.pdf") != 2 {{ error("page_break tenía que dar 2 páginas") }}
        t = pdf.read("{dir}/b.pdf")
        for trozo in ["Factura F-1", "Íñigo Muñoz", "Detalle", "179,80 €", "Un texto suelto", "Segunda página", "pie propio"] {{
            if not t.contains(trozo) {{ error("falta en el PDF: " + trozo) }}
        }}
        if pdf.info("{dir}/b.pdf")["author"] != "Comercio" {{ error("metadato author") }}
    "##));
}

#[test]
fn build_rechaza_un_bloque_desconocido() {
    let (_d, dir) = carpeta("build_malo");
    let err = run_err(&format!(r##"
        use "pdf" as pdf
        pdf.build("{dir}/x.pdf", [{{ "cosa": 1 }}])
    "##));
    assert!(err.contains("unknown block"), "{err}");
}

//   Edición de PDF existentes                         

/// Dos PDF de partida: uno de 3 páginas con "PAGINA-n" en cada una y otro de 1.
fn fuentes(dir: &str) -> String {
    format!(r##"
        use "pdf" as pdf
        pdf.build("{dir}/tres.pdf", [
            {{ "text": "PAGINA-1" }}, {{ "page_break": yes }},
            {{ "text": "PAGINA-2" }}, {{ "page_break": yes }},
            {{ "text": "PAGINA-3" }}
        ], {{ "page_numbers": no }})
        pdf.build("{dir}/uno.pdf", [{{ "text": "OTRO-DOC" }}], {{ "page_numbers": no }})
    "##)
}

#[test]
fn merge_delete_reorder_rotate() {
    let (_d, dir) = carpeta("paginas");
    run_ok(&format!(r##"
        {}
        pdf.merge(["{dir}/tres.pdf", "{dir}/uno.pdf"], "{dir}/m.pdf")
        if pdf.pages("{dir}/m.pdf") != 4 {{ error("merge: 3 + 1 páginas") }}
        t = pdf.read("{dir}/m.pdf")
        if not t.contains("PAGINA-3") {{ error("merge perdió texto del primero") }}
        if not t.contains("OTRO-DOC") {{ error("merge perdió texto del segundo") }}

        pdf.delete_pages("{dir}/m.pdf", "{dir}/d.pdf", "2-3")
        if pdf.pages("{dir}/d.pdf") != 2 {{ error("delete_pages 2-3") }}
        if pdf.read("{dir}/d.pdf").contains("PAGINA-2") {{ error("la página 2 seguía ahí") }}

        pdf.delete_pages("{dir}/m.pdf", "{dir}/d2.pdf", -1)
        if pdf.read("{dir}/d2.pdf").contains("OTRO-DOC") {{ error("-1 tenía que quitar la última") }}

        pdf.reorder("{dir}/tres.pdf", "{dir}/o.pdf", [3, 1, 2])
        t = pdf.read("{dir}/o.pdf")
        if t.find("PAGINA-3") > t.find("PAGINA-1") {{ error("reorder: la 3 tenía que ir primero") }}

        pdf.rotate("{dir}/tres.pdf", "{dir}/r.pdf", 90, 1)
        pdf.rotate("{dir}/r.pdf", "{dir}/r2.pdf", 270, "all")
    "##, fuentes(&dir)));

    // Rotate: la 1 da la vuelta completa (90 + 270 = 360 → 0), las demás 270.
    let doc = lopdf::Document::load(format!("{dir}/r2.pdf")).unwrap();
    let giros: Vec<i64> = doc.get_pages().values().map(|id| {
        doc.get_dictionary(*id).unwrap().get(b"Rotate").and_then(|o| o.as_i64()).unwrap_or(0)
    }).collect();
    assert_eq!(giros, vec![0, 270, 270]);
}

#[test]
fn errores_claros_al_editar() {
    let (_d, dir) = carpeta("errores");
    run_ok(&fuentes(&dir));
    for (codigo, esperado) in [
        (format!(r##"use "pdf" as pdf
pdf.delete_pages("{dir}/tres.pdf", "{dir}/x.pdf", "1-3")"##), "at least one page"),
        (format!(r##"use "pdf" as pdf
pdf.rotate("{dir}/tres.pdf", "{dir}/x.pdf", 45)"##), "multiple of 90"),
        (format!(r##"use "pdf" as pdf
pdf.reorder("{dir}/tres.pdf", "{dir}/x.pdf", [1, 1])"##), "appears twice"),
        (format!(r##"use "pdf" as pdf
pdf.delete_pages("{dir}/tres.pdf", "{dir}/x.pdf", 9)"##), "does not exist"),
    ] {
        let err = run_err(&codigo);
        assert!(err.contains(esperado), "esperaba '{esperado}', salió: {err}");
    }
}

#[test]
fn stamp_numera_y_no_borra_el_texto() {
    let (_d, dir) = carpeta("stamp");
    run_ok(&format!(r##"
        {}
        pdf.stamp("{dir}/tres.pdf", "{dir}/n.pdf", "Hoja {{page}} de {{pages}}", {{ "position": "bottom-center" }})
        t = pdf.read("{dir}/n.pdf")
        for trozo in ["Hoja 1 de 3", "Hoja 3 de 3", "PAGINA-1", "PAGINA-3"] {{
            if not t.contains(trozo) {{ error("falta tras stamp: " + trozo) }}
        }}
        pdf.stamp("{dir}/tres.pdf", "{dir}/p.pdf", "PAGADO", {{ "pages": 2, "rotation": 20, "opacity": 0.5, "color": "red" }})
        t = pdf.read("{dir}/p.pdf")
        if len(t.split("PAGADO")) - 1 != 1 {{ error("pages: 2 tenía que estampar una sola página") }}
    "##, fuentes(&dir)));
}

#[test]
fn watermark_conserva_las_fuentes_de_la_pagina() {
    let (_d, dir) = carpeta("marca");
    run_ok(&format!(r##"
        {}
        pdf.watermark("{dir}/tres.pdf", "{dir}/w.pdf", "BORRADOR · año 2026")
        t = pdf.read("{dir}/w.pdf")
        if not t.contains("PAGINA-2") {{ error("la marca de agua se comió el texto") }}
        if not t.contains("año") {{ error("la marca no codificó bien las tildes") }}
    "##, fuentes(&dir)));

    // Cada página tiene que conservar sus fuentes y además la de la marca.
    let doc = lopdf::Document::load(format!("{dir}/w.pdf")).unwrap();
    for id in doc.get_pages().values() {
        let pagina = doc.get_dictionary(*id).unwrap();
        let recursos = pagina.get(b"Resources").unwrap().as_dict().unwrap();
        let fuentes = recursos.get(b"Font").unwrap().as_dict().unwrap();
        assert!(fuentes.len() >= 2, "la página se quedó solo con la fuente de la marca: {fuentes:?}");
        assert!(fuentes.has(b"OrionF"));
    }
}

#[test]
fn stamp_image_y_metadatos_con_tildes() {
    let (d, dir) = carpeta("imagen_info");
    logo(&d);
    run_ok(&format!(r##"
        {}
        pdf.stamp_image("{dir}/tres.pdf", "{dir}/l.pdf", "{dir}/logo.png", {{ "position": "top-left", "width": 20 }})
        if pdf.pages("{dir}/l.pdf") != 3 {{ error("stamp_image cambió el número de páginas") }}

        pdf.set_info("{dir}/tres.pdf", "{dir}/i.pdf", {{ "title": "Factura de Íñigo", "keywords": "a, b" }})
        info = pdf.info("{dir}/i.pdf")
        if info["title"] != "Factura de Íñigo" {{ error("title con tildes: " + str(info["title"])) }}
        if info["keywords"] != "a, b" {{ error("keywords") }}
        if info["pages"] != 3 {{ error("info.pages") }}
    "##, fuentes(&dir)));

    let doc = lopdf::Document::load(format!("{dir}/l.pdf")).unwrap();
    let id = *doc.get_pages().values().next().unwrap();
    let recursos = doc.get_dictionary(id).unwrap().get(b"Resources").unwrap().as_dict().unwrap();
    assert!(recursos.get(b"XObject").unwrap().as_dict().unwrap().has(b"OrionImg"));
}

//   Orden de columnas en Excel y CSV                     ─

#[test]
fn excel_write_styled_respeta_el_orden_de_las_claves() {
    let (_d, dir) = carpeta("excel");
    run_ok(&format!(r##"
        use "excel" as excel
        filas = [{{ "Fecha": "2026-09-01", "Pedidos": 3, "Importe": 10.5 }},
                 {{ "Fecha": "2026-09-02", "Pedidos": 1, "Importe": 2, "Nota": "solo aquí" }}]
        excel.write_styled("{dir}/e.xlsx", filas, {{ "alternar": yes }})
        h = excel.sheet("{dir}/e.xlsx")["headers"]
        if str(h) != str(["Fecha", "Pedidos", "Importe", "Nota"]) {{ error("orden de columnas: " + str(h)) }}
    "##));
}

#[test]
fn csv_write_respeta_el_orden_y_todas_las_claves() {
    let (_d, dir) = carpeta("csv");
    run_ok(&format!(r##"
        use "csv" as csv
        use "fs" as fs
        filas = [{{ "z": 1, "a": 2 }}, {{ "z": 3, "a": 4, "m": 5 }}]
        csv.write("{dir}/c.csv", filas)
        primera = fs.read("{dir}/c.csv").split("\n")[0].trim()
        if primera != "z,a,m" {{ error("cabecera del CSV: " + primera) }}
        if str(csv.headers(filas)) != str(["z", "a", "m"]) {{ error("csv.headers: " + str(csv.headers(filas))) }}
    "##));
}
