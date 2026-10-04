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
    [string]$Dir = $PSScriptRoot,
    # Guarda stderr en err_<Etiqueta>.txt en vez de mostrarlo.
    [switch]$Stderr,
    # Segundos antes de cortar el proceso (0 = sin límite).
    [int]$LimiteSeg = 0
)
$redir = @{ RedirectStandardOutput = "$Dir\out_$Etiqueta.txt" }
if ($Stderr) { $redir.RedirectStandardError = "$Dir\err_$Etiqueta.txt" }
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -ArgumentList $Args -WorkingDirectory $Dir `
     -NoNewWindow -PassThru @redir
$peak = 0
$cortado = $false
while (-not $p.HasExited) {
    if ($LimiteSeg -gt 0 -and $sw.Elapsed.TotalSeconds -gt $LimiteSeg) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        $cortado = $true
        break
    }
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
$extra = if ($cortado) { " cortado_al_limite" } else { "" }
Write-Output ("[{0}] pared_ms={1} pico_RAM_MB={2}{3}" -f $Etiqueta, $sw.ElapsedMilliseconds, $peakMB, $extra)
Get-Content "$Dir\out_$Etiqueta.txt"
