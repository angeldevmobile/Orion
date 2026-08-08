# Como `medir.ps1`, pero suma la RAM de los procesos AUXILIARES que arranca
# cada pila, no solo la del proceso que escribes tú.
#
# Hace falta porque medir solo el proceso principal favorece sin querer a las
# herramientas que delegan trabajo en otro proceso:
#
#   Orion       -> ninguno. Habla CDP desde su propio proceso.
#   Selenium    -> chromedriver.exe, un segundo binario cuya version tiene que
#                  corresponderse con la de Chrome.
#   Playwright  -> un node.exe, porque su driver esta escrito en JavaScript.
#
# Ese proceso no es el navegador y no es igual para las tres, asi que dejarlo
# fuera de la cuenta era contar mal. El navegador si se excluye: es el mismo
# binario y el mismo trabajo en los tres casos.
#
# Los auxiliares se identifican por PID contra una foto tomada justo antes de
# arrancar: en una maquina de desarrollo hay procesos `node` de otras cosas
# —el editor, sin ir mas lejos— y contarlos falsearia el resultado.
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Args,
    [Parameter(Mandatory=$true)][string]$Etiqueta,
    [string]$Dir = $PSScriptRoot
)

$AUXILIARES = @("chromedriver", "msedgedriver", "geckodriver", "node")

$previos = @{}
Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $AUXILIARES -contains $_.Name } |
    ForEach-Object { $previos[$_.Id] = $true }

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -ArgumentList $Args -WorkingDirectory $Dir `
     -NoNewWindow -PassThru -RedirectStandardOutput "$Dir\out_$Etiqueta.txt"

$picoPrincipal = 0
$picoTotal     = 0
$vistos        = @{}

while (-not $p.HasExited) {
    try {
        $p.Refresh()
        $principal = $p.WorkingSet64
        if ($principal -gt $picoPrincipal) { $picoPrincipal = $principal }

        $aux = 0
        Get-Process -ErrorAction SilentlyContinue |
            Where-Object { $AUXILIARES -contains $_.Name -and -not $previos.ContainsKey($_.Id) } |
            ForEach-Object { $aux += $_.WorkingSet64; $vistos[$_.Name] = $true }

        if (($principal + $aux) -gt $picoTotal) { $picoTotal = $principal + $aux }
    } catch {}
    Start-Sleep -Milliseconds 10
}
$sw.Stop()

$quienes = if ($vistos.Count) { ($vistos.Keys | Sort-Object) -join "+" } else { "ninguno" }
Write-Output ("[{0}] pared_ms={1} pico_proceso_MB={2} pico_pila_MB={3} auxiliares={4}" -f `
    $Etiqueta, $sw.ElapsedMilliseconds,
    [math]::Round($picoPrincipal / 1MB, 1),
    [math]::Round($picoTotal / 1MB, 1),
    $quienes)
Get-Content "$Dir\out_$Etiqueta.txt"
