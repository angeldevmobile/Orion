# Intérprete (--run) contra JIT (--jit): tiempo de pared y pico de RAM.
# Una corrida de calentamiento y luego -Veces medidas; se reporta la más rápida,
# porque el ruido del equipo entre corridas llega al 40 %.
param(
    [string]$Orion = (Join-Path $PSScriptRoot "..\..\orion-vm\target\release\orion.exe"),
    [string[]]$Bench = @("bucle_int", "bucle_main", "bucle_float", "fib", "lista", "ternario"),
    # Una medida que pasa de aquí se corta y sale como "cortado".
    [int]$LimiteSeg = 90,
    [int]$Veces = 3
)
$ErrorActionPreference = "Stop"
$aqui  = $PSScriptRoot
$medir = Join-Path $aqui "..\medir.ps1"

if (-not (Test-Path $Orion)) {
    Write-Host "Falta el binario release. Compila con:"
    Write-Host "  cargo build --release --manifest-path orion-vm/Cargo.toml"
    exit 1
}

$filas = @()
foreach ($b in $Bench) {
    foreach ($modo in "run", "jit") {
        $et = "${b}_$modo"
        Write-Host "-- $b --$modo"
        $med = @{ Exe = $Orion; Args = "--$modo $b.orx"; Etiqueta = $et; Dir = $aqui
                  Stderr = $true; LimiteSeg = $LimiteSeg }
        $linea = & $medir @med | Select-Object -First 1
        $ms, $ram = [int]::MaxValue, ""
        # La primera es de calentamiento; si ya se cortó no se repite.
        for ($i = 0; $i -lt $Veces -and $linea -notmatch "cortado"; $i++) {
            $linea = & $medir @med | Select-Object -First 1
            $null = $linea -match "pared_ms=(\d+) pico_RAM_MB=([\d.,]+)"
            if ([int]$Matches[1] -lt $ms) { $ms, $ram = [int]$Matches[1], $Matches[2] }
        }
        if ($linea -match "cortado") {
            $null = $linea -match "pared_ms=(\d+) pico_RAM_MB=([\d.,]+)"
            $ms, $ram = [int]$Matches[1], $Matches[2]
        }
        $err = Get-Content "$aqui\err_$et.txt" -Raw
        # --jit avisa por stderr si compiló a nativo o cayó al intérprete.
        $motor = if ($linea -match "cortado") { "cortado a ${LimiteSeg}s" }
                 elseif ($modo -eq "run") { "interprete" }
                 elseif ($err -match "falling back") { "JIT->interprete" }
                 elseif ($err -match "Cranelift nativo") { "nativo" }
                 else { "error" }
        $filas += [pscustomobject]@{
            Bench    = $b
            Motor    = $motor
            Pared_ms = $ms
            RAM_MB   = $ram
            Salida   = "$(Get-Content "$aqui\out_$et.txt" -Raw)".Trim()
        }
    }
}

$filas | Format-Table -AutoSize
foreach ($b in $Bench) {
    $s = @($filas | Where-Object Bench -eq $b | ForEach-Object Salida)
    if ($s[0] -ne $s[1]) { Write-Host "AVISO: $b da salida distinta en --run y --jit" }
    $e = (Get-Content "$aqui\err_${b}_jit.txt" -Raw)
    if ($e -match "error|Error") { Write-Host "AVISO: $b en --jit escribio en stderr:`n$e" }
}
