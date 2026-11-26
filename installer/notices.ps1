param([string]$Toolchain, [string]$Target, [string]$Version, [string]$Stage)
$raw = & cargo "+$Toolchain" metadata --locked --format-version 1 --filter-platform $Target
if ($LASTEXITCODE -ne 0) { throw "Could not collect dependency licenses" }
$meta = $raw | Out-String | ConvertFrom-Json
$used = @{}
foreach ($n in $meta.resolve.nodes) { $used[$n.id] = $true }
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("Voxa $version - third-party software notices")
[void]$sb.AppendLine("Voxa includes the following open source components. Their license texts follow.")
[void]$sb.AppendLine()
$seen = @{}
$sha = [Security.Cryptography.SHA256]::Create()
foreach ($p in ($meta.packages | Where-Object { $_.source -and $used[$_.id] } | Sort-Object name, version)) {
    [void]$sb.AppendLine('=' * 78)
    [void]$sb.AppendLine("$($p.name) $($p.version)  -  License: $($p.license)")
    if ($p.repository) { [void]$sb.AppendLine($p.repository) }
    [void]$sb.AppendLine('=' * 78)
    $dir = Split-Path $p.manifest_path
    $files = Get-ChildItem $dir -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT)' }
    foreach ($f in $files) {
        $text = Get-Content $f.FullName -Raw
        $hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($text)))
        if ($seen.ContainsKey($hash)) {
            [void]$sb.AppendLine("[$($f.Name): identical to the text shown for $($seen[$hash])]")
        } else {
            $seen[$hash] = "$($p.name) ($($f.Name))"
            [void]$sb.AppendLine("--- $($f.Name) ---")
            [void]$sb.AppendLine($text.Trim())
        }
        [void]$sb.AppendLine()
    }
    [void]$sb.AppendLine()
}
[IO.File]::WriteAllText((Join-Path $stage 'THIRD-PARTY-NOTICES.txt'), $sb.ToString())
