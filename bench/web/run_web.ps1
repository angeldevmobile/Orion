# Benchmark de automatización web: Orion contra Selenium y Playwright.
#
#   powershell -ExecutionPolicy Bypass -File bench\web\run_web.ps1 [filas]
#
# Requisitos: python con `selenium` y `playwright` instalados, Chrome instalado,
# y el binario release de Orion.
#
# Reglas de la comparación, para que el número signifique algo:
#   - Las tres herramientas mueven EL MISMO ejecutable de Chrome.
#   - Las tres en headless y sin imágenes.
#   - Las tres cargan el mismo archivo local: no hay red ni servidor de por medio.
#   - Cada una imprime una huella de lo extraído; si no coinciden, el
#     benchmark se detiene en vez de publicar tiempos de tareas distintas.
#
# Se miden DOS cosas por separado, porque mezclarlas engaña:
#   - extracción: la lectura repetida, y se coge el MEJOR tiempo. El ruido solo
#     puede sumar, así que el mínimo es el que más se acerca al coste real.
#   - proceso entero: una sola extracción, con arranque incluido. Si aquí dentro
#     se repitiera, el "tiempo de pared" mediría la repetición y no la tarea.

$ErrorActionPreference = "Stop"
$aqui  = $PSScriptRoot
$raiz  = Split-Path (Split-Path $aqui -Parent) -Parent
$medir = Join-Path (Split-Path $aqui -Parent) "medir.ps1"
$pila  = Join-Path $aqui "medir_pila.ps1"
$orion = Join-Path $raiz "orion-vm\target\release\orion.exe"

if (-not (Test-Path $orion)) {
    Write-Host "Falta el binario release. Compila con:" -ForegroundColor Yellow
    Write-Host "  cargo build --release --manifest-path orion-vm/Cargo.toml"
    exit 1
}

$filas = if ($args.Count -gt 0) { $args[0] } else { 500 }
Write-Host "== Pagina de prueba ==" -ForegroundColor Cyan
python (Join-Path $aqui "gen_pagina.py") $filas

$casos = @(
    @{ Etiqueta = "selenium_idiomatico";   Exe = "python"; Args = "bench_selenium.py idiomatico" },
    @{ Etiqueta = "selenium_js";           Exe = "python"; Args = "bench_selenium.py js" },
    @{ Etiqueta = "playwright_idiomatico"; Exe = "python"; Args = "bench_playwright.py idiomatico" },
    @{ Etiqueta = "playwright_js";         Exe = "python"; Args = "bench_playwright.py js" },
    @{ Etiqueta = "orion_extract";         Exe = $orion;   Args = "run bench_orion.orx" }
)

function Sacar($lineas, $patron) {
    foreach ($l in $lineas) { if ($l -match $patron) { return $Matches[1] } }
    return $null
}

$res = @{}

Write-Host ""
Write-Host "== 1/2  Extraccion (repetida, mejor tiempo) ==" -ForegroundColor Cyan
Remove-Item Env:\BENCH_UNA -ErrorAction SilentlyContinue
foreach ($c in $casos) {
    $s = & $medir -Exe $c.Exe -Args $c.Args -Etiqueta $c.Etiqueta -Dir $aqui
    $s | ForEach-Object { Write-Host "  $_" }
    $res[$c.Etiqueta] = @{
        extraccion = Sacar $s "extraccion_ms=([0-9.]+)"
        vueltas    = Sacar $s "vueltas=(\d+)"
        huella     = Sacar $s "huella=(\w+)"
    }
}

Write-Host ""
Write-Host "== 2/2  Proceso entero (una extraccion, arranque incluido) ==" -ForegroundColor Cyan
$env:BENCH_UNA = "1"
foreach ($c in $casos) {
    $s = & $pila -Exe $c.Exe -Args $c.Args -Etiqueta ("una_" + $c.Etiqueta) -Dir $aqui
    $s | ForEach-Object { Write-Host "  $_" }
    $res[$c.Etiqueta].pared = Sacar $s "pared_ms=(\d+)"
    $res[$c.Etiqueta].ram   = Sacar $s "pico_proceso_MB=([0-9.]+)"
    $res[$c.Etiqueta].pila  = Sacar $s "pico_pila_MB=([0-9.]+)"
    $res[$c.Etiqueta].aux   = Sacar $s "auxiliares=(\S+)"
}
Remove-Item Env:\BENCH_UNA -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "== Verificacion: todos extrajeron lo mismo ==" -ForegroundColor Cyan
$distintas = @($res.Values | ForEach-Object { $_.huella } | Sort-Object -Unique)
if ($distintas.Count -eq 1) {
    Write-Host ("OK - huella comun: " + $distintas[0]) -ForegroundColor Green
} else {
    Write-Host "NO COINCIDEN - los tiempos no son comparables:" -ForegroundColor Red
    $res.GetEnumerator() | ForEach-Object { Write-Host ("  {0} -> {1}" -f $_.Key, $_.Value.huella) }
    exit 1
}

Write-Host ""
Write-Host ("== Resultados ({0} filas x 4 campos) ==" -f $filas) -ForegroundColor Cyan
Write-Host ""
Write-Host ("{0,-24} {1,12} {2,10} {3,10} {4,10} {5,-14}" -f "variante", "extraccion", "pared", "proceso", "pila", "auxiliares")
Write-Host ("{0,-24} {1,12} {2,10} {3,10} {4,10} {5,-14}" -f ("-"*24), ("-"*12), ("-"*10), ("-"*10), ("-"*10), ("-"*14))
foreach ($c in $casos) {
    $r = $res[$c.Etiqueta]
    Write-Host ("{0,-24} {1,12} {2,10} {3,10} {4,10} {5,-14}" -f `
        $c.Etiqueta, ("{0} ms" -f $r.extraccion), ("{0} ms" -f $r.pared),
        ("{0} MB" -f $r.ram), ("{0} MB" -f $r.pila), $r.aux)
}
Write-Host ""
Write-Host "extraccion = mejor de N vueltas | el resto = proceso entero con 1 extraccion"
Write-Host "proceso = solo el que escribes tu | pila = ese mas los auxiliares que arranca"
Write-Host "El navegador NO se cuenta: es el mismo binario y el mismo trabajo en los tres."
