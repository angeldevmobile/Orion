# Corre un comando y reporta tiempo de pared + pico de RAM (working set).
#
# El pico se muestrea cada 10ms mientras el proceso vive: PeakWorkingSet64
# leído tras la salida del proceso devuelve 0, por eso el polling.
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Args,
    [Parameter(Mandatory=$true)][string]$Etiqueta,
    # Dónde corre y dónde deja su salida. Por defecto, junto a este script:
    # así los benchmarks que ya existían no cambian de comportamiento.
    [string]$Dir = $PSScriptRoot
)
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -ArgumentList $Args -WorkingDirectory $Dir `
     -NoNewWindow -PassThru -RedirectStandardOutput "$Dir\out_$Etiqueta.txt"
$peak = 0
while (-not $p.HasExited) {
    try {
        $p.Refresh()
        if ($p.WorkingSet64 -gt $peak) { $peak = $p.WorkingSet64 }
    } catch {}
    Start-Sleep -Milliseconds 10
}
$sw.Stop()
$peakMB = [math]::Round($peak / 1MB, 1)
# Al flujo de salida y no a la consola: así un script que orqueste varias
# medidas puede capturar la línea y montar una tabla. Sin capturar se sigue
# viendo igual.
Write-Output ("[{0}] pared_ms={1} pico_RAM_MB={2}" -f $Etiqueta, $sw.ElapsedMilliseconds, $peakMB)
Get-Content "$Dir\out_$Etiqueta.txt"
