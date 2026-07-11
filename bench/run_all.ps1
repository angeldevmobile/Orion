# Benchmark completo Orion vs Python — un solo comando:
#
#   powershell -ExecutionPolicy Bypass -File bench\run_all.ps1
#
# Requisitos: python en el PATH y el binario release compilado
# (cargo build --release --manifest-path orion-vm/Cargo.toml).
#
# Metodología: cada medición se corre DOS veces y se reporta la segunda
# (en caliente, con el archivo en el cache del SO) para que ninguno pague
# el I/O frío. La primera corrida se descarta.
$ErrorActionPreference = "Stop"
$aqui  = $PSScriptRoot
$orion = Join-Path $aqui "..\orion-vm\target\release\orion.exe"
$medir = Join-Path $aqui "medir.ps1"

if (-not (Test-Path $orion)) {
    Write-Host "Falta el binario release. Compila con:"
    Write-Host "  cargo build --release --manifest-path orion-vm/Cargo.toml"
    exit 1
}

if (-not (Test-Path (Join-Path $aqui "data.csv"))) {
    Write-Host "== Generando dataset (500k filas) =="
    python (Join-Path $aqui "gen_data.py")
}

Write-Host "`n== Python stdlib: CSV -> columnas tipadas + sum + mean =="
& $medir -Exe "python" -Args "bench_py.py" -Etiqueta "py_descartada" | Out-Null
& $medir -Exe "python" -Args "bench_py.py" -Etiqueta "python_csv"

Write-Host "`n== Orion: frame.open CSV + sum + mean =="
& $medir -Exe $orion -Args "bench_csv.orx" -Etiqueta "orion_descartada" | Out-Null
& $medir -Exe $orion -Args "bench_csv.orx" -Etiqueta "orion_csv"

Write-Host "`n== Conversion CSV -> .odf (una vez) =="
& $medir -Exe $orion -Args "conv_odf.orx" -Etiqueta "conv_odf"

Write-Host "`n== Orion: frame.open .odf + sum + mean =="
& $medir -Exe $orion -Args "bench_odf.orx" -Etiqueta "odf_descartada" | Out-Null
& $medir -Exe $orion -Args "bench_odf.orx" -Etiqueta "orion_odf"

Write-Host "`n== Estres del GC: 200k listas ciclicas huerfanas =="
& $medir -Exe $orion -Args "gc_ciclos.orx" -Etiqueta "gc_ciclos"

Write-Host "`nListo. Compara pared_ms / pico_RAM_MB de python_csv, orion_csv y orion_odf."
