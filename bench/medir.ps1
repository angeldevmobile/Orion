# Corre un comando y reporta tiempo de pared + pico de RAM (working set).
#
# El pico se muestrea cada 10ms mientras el proceso vive: PeakWorkingSet64
# leído tras la salida del proceso devuelve 0, por eso el polling.
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Args,
    [Parameter(Mandatory=$true)][string]$Etiqueta
)
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -ArgumentList $Args -WorkingDirectory $PSScriptRoot `
     -NoNewWindow -PassThru -RedirectStandardOutput "$PSScriptRoot\out_$Etiqueta.txt"
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
Write-Host ("[{0}] pared_ms={1} pico_RAM_MB={2}" -f $Etiqueta, $sw.ElapsedMilliseconds, $peakMB)
Get-Content "$PSScriptRoot\out_$Etiqueta.txt"
