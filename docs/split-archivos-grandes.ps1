<#
Fragmenta en partes de 1900 MiB (por debajo del límite de 2 GiB de GitHub Releases)
los archivos del cliente WarCrafted que superan ese límite, según la decisión 0003
(docs/decisiones/0003-fragmentacion-archivos-grandes.md).

Uso: edita $clienteDir si tu ruta es distinta, y ejecuta desde PowerShell:
    .\split-archivos-grandes.ps1

Genera, junto a cada archivo original, sus fragmentos "<archivo>.part-000", "part-001", etc.
No modifica ni borra el archivo original.
#>

$clienteDir = "C:\Wow\Per pujar\WotLK-WarCrafted\Data"
[int64]$partSizeBytes = 1900 * 1024 * 1024  # 1900 * 1024 * 1024 bytes

$archivosAFragmentar = @(
    "patch.MPQ",
    "common.MPQ",
    "lichking.MPQ"
)

foreach ($nombre in $archivosAFragmentar) {
    $origen = Join-Path $clienteDir $nombre
    if (-not (Test-Path $origen)) {
        Write-Warning "No encontrado: $origen (saltando)"
        continue
    }

    [int64]$tamanoTotal = (Get-Item $origen).Length
    [int64]$numPartes = $tamanoTotal -div $partSizeBytes
    if (($tamanoTotal % $partSizeBytes) -ne 0) { $numPartes++ }
    Write-Host "Fragmentando $nombre ($tamanoTotal bytes) en $numPartes partes..."

    # Read() y el tamaño de un array .NET reciben un Int32; el resto de
    # tamaños y posiciones de archivo se mantienen como Int64.
    [int]$bufferSize = 64MB
    $buffer = New-Object byte[] $bufferSize
    $origenStream = [System.IO.File]::OpenRead($origen)
    try {
        for ([int64]$parte = 0; $parte -lt $numPartes; $parte++) {
            $sufijo = $parte.ToString("000")
            $destino = "$origen.part-$sufijo"
            [int64]$bytesDisponibles = $tamanoTotal - [int64]$origenStream.Position
            [int64]$restante = if ($bytesDisponibles -lt $partSizeBytes) { $bytesDisponibles } else { $partSizeBytes }
            $destinoStream = [System.IO.File]::Create($destino)
            try {
                while ($restante -gt 0) {
                    [int]$aLeer = if ($restante -lt [int64]$bufferSize) { [int]$restante } else { $bufferSize }
                    [int]$leidos = $origenStream.Read($buffer, 0, $aLeer)
                    if ($leidos -le 0) { break }
                    $destinoStream.Write($buffer, 0, $leidos)
                    $restante -= $leidos
                }
            } finally {
                $destinoStream.Close()
            }
            Write-Host "  -> $destino"
        }
    } finally {
        $origenStream.Close()
    }

    Write-Host "Hash SHA-256 de cada parte de $nombre (guárdalos para el manifest):"
    Get-ChildItem "$origen.part-*" | Sort-Object Name | ForEach-Object {
        $hash = Get-FileHash $_.FullName -Algorithm SHA256
        Write-Host "  $($_.Name): $($hash.Hash.ToLower())"
    }

    Write-Host "Hash SHA-256 del archivo original completo ($nombre, va en files[].sha256):"
    $hashOriginal = Get-FileHash $origen -Algorithm SHA256
    Write-Host "  $($hashOriginal.Hash.ToLower())"
    $carpetaFragmentos = (Get-Item $origen).Directory.FullName
    Write-Host "Fragmentos de $nombre en: $carpetaFragmentos"
    Write-Host ""
}

Write-Host "Listo. Verifica que la suma de tamaños de las partes de cada archivo coincide con el original antes de subir nada."
