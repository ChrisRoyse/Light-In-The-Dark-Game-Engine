param(
    [string]$OutputPath = "packaging/windows/litd.ico"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")
Assert-LitdWindowsPlatform

$output = if ([IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
}
$parent = Split-Path -Parent $output
if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
    throw "LITD_WINDOWS_ICON_OUTPUT_PARENT_MISSING: output directory not found at $parent"
}

Add-Type -AssemblyName System.Drawing
$bitmap = [Drawing.Bitmap]::new(256, 256, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$png = [IO.MemoryStream]::new()
try {
    $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([Drawing.Color]::FromArgb(255, 13, 18, 28))

    $rim = [Drawing.Pen]::new([Drawing.Color]::FromArgb(255, 112, 128, 151), 8)
    $gold = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(255, 235, 181, 77))
    $ember = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(255, 216, 83, 46))
    $dark = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(255, 23, 31, 46))
    try {
        $graphics.DrawEllipse($rim, 24, 24, 208, 208)
        $flame = [Drawing.PointF[]]@(
            [Drawing.PointF]::new(128, 43),
            [Drawing.PointF]::new(181, 113),
            [Drawing.PointF]::new(171, 179),
            [Drawing.PointF]::new(128, 216),
            [Drawing.PointF]::new(85, 179),
            [Drawing.PointF]::new(75, 113)
        )
        $graphics.FillPolygon($gold, $flame)
        $inner = [Drawing.PointF[]]@(
            [Drawing.PointF]::new(128, 88),
            [Drawing.PointF]::new(154, 131),
            [Drawing.PointF]::new(146, 174),
            [Drawing.PointF]::new(128, 190),
            [Drawing.PointF]::new(110, 174),
            [Drawing.PointF]::new(102, 131)
        )
        $graphics.FillPolygon($ember, $inner)
        $graphics.FillEllipse($dark, 116, 142, 24, 42)
    } finally {
        $rim.Dispose()
        $gold.Dispose()
        $ember.Dispose()
        $dark.Dispose()
    }
    $bitmap.Save($png, [Drawing.Imaging.ImageFormat]::Png)
} finally {
    $graphics.Dispose()
    $bitmap.Dispose()
}

$pngBytes = $png.ToArray()
$png.Dispose()
$file = [IO.File]::Open($output, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
$writer = [IO.BinaryWriter]::new($file)
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]1)
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]32)
    $writer.Write([uint32]$pngBytes.Length)
    $writer.Write([uint32]22)
    $writer.Write($pngBytes)
} finally {
    $writer.Dispose()
}

$item = Get-Item -LiteralPath $output
[pscustomobject]@{
    path = $item.FullName
    bytes = $item.Length
    sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    dimensions = "256x256"
    encoding = "png-in-ico"
}
