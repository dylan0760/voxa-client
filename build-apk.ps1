# Builds the Android app: dist\Voxa-<version>.apk (arm64 phones and tablets, plus x86_64
# for Chromebooks and the emulator).
#
# Needs: Rust with the aarch64-linux-android and x86_64-linux-android targets, the
# Android NDK (ANDROID_NDK_HOME), the Android SDK (ANDROID_HOME) with build-tools and
# platforms;android-35, and Java (JAVA_HOME) for apksigner.
#
# Signing: the same key must sign every version, or Android refuses to update the app.
# Give it with VOXA_APK_KEYSTORE (path to the .jks) and VOXA_APK_KEYPASS.
#   .\build-apk.ps1               build and package
#   .\build-apk.ps1 -SkipCargo    only re-package from the last build
param([switch]$SkipCargo, [string]$Toolchain = $(if ($env:VOXA_TOOLCHAIN) { $env:VOXA_TOOLCHAIN } else { 'stable' }))
# Not 'Stop': Windows PowerShell treats Cargo's progress output as errors. Every step
# below checks its own result.
$ErrorActionPreference = 'Continue'
$Root = $PSScriptRoot
$Crate = Join-Path $Root 'crates\voxa-android'
if (-not $env:ANDROID_HOME) { throw 'ANDROID_HOME is not set (the Android SDK)' }
if (-not $env:ANDROID_NDK_HOME) { throw 'ANDROID_NDK_HOME is not set (the Android NDK)' }
if (-not $env:VOXA_APK_KEYSTORE -or -not (Test-Path $env:VOXA_APK_KEYSTORE)) { throw 'VOXA_APK_KEYSTORE must point to the signing key' }
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'voxa-build-target\voxa-android' }

$version = (Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$parts = $version.Split('.') | ForEach-Object { [int]$_ }
$code = $parts[0] * 1000000 + $parts[1] * 1000 + $parts[2]

# The NDK's compilers for the C parts of some crates, and as the linker.
$hostTag = if ($IsLinux) { 'linux-x86_64' } else { 'windows-x86_64' }
$tc = Join-Path $env:ANDROID_NDK_HOME "toolchains\llvm\prebuilt\$hostTag\bin"
$ext = if ($IsLinux) { '' } else { '.cmd' }
$abis = @(@{ Target = 'aarch64-linux-android'; Abi = 'arm64-v8a' }, @{ Target = 'x86_64-linux-android'; Abi = 'x86_64' })
foreach ($a in $abis) {
    $t = $a.Target
    $var = $t.ToUpper().Replace('-', '_')
    $clang = Join-Path $tc "$($t)26-clang$ext"
    Set-Item "env:CC_$($t.Replace('-', '_'))" $clang
    Set-Item "env:AR_$($t.Replace('-', '_'))" (Join-Path $tc "llvm-ar$(if ($IsLinux) { '' } else { '.exe' })")
    Set-Item "env:CARGO_TARGET_$($var)_LINKER" $clang
}

Push-Location $Root
try {
    if (-not $SkipCargo) {
        foreach ($a in $abis) {
            & cargo "+$Toolchain" build --release --locked -p voxa-android --target $a.Target
            if ($LASTEXITCODE -ne 0) { throw "cargo build for $($a.Target) failed" }
        }
    }

    $bt = Get-ChildItem (Join-Path $env:ANDROID_HOME 'build-tools') -Directory | Sort-Object { [version]($_.Name -replace '[^0-9.]', '') } | Select-Object -Last 1
    $platform = Get-ChildItem (Join-Path $env:ANDROID_HOME 'platforms') -Directory | Where-Object { $_.Name -match '^android-\d+$' } | Sort-Object { [int]($_.Name -replace 'android-', '') } | Select-Object -Last 1
    $exe = if ($IsLinux) { '' } else { '.exe' }
    $bat = if ($IsLinux) { '' } else { '.bat' }
    $aapt2 = Join-Path $bt.FullName "aapt2$exe"
    $zipalign = Join-Path $bt.FullName "zipalign$exe"
    $apksigner = Join-Path $bt.FullName "apksigner$bat"
    $androidJar = Join-Path $platform.FullName 'android.jar'

    $work = Join-Path $env:CARGO_TARGET_DIR 'apk'
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory $work | Out-Null
    $manifest = (Get-Content (Join-Path $Crate 'AndroidManifest.xml') -Raw).Replace('@VERSION_CODE@', "$code").Replace('@VERSION_NAME@', $version)
    [IO.File]::WriteAllText((Join-Path $work 'AndroidManifest.xml'), $manifest, (New-Object System.Text.UTF8Encoding($false)))

    & $aapt2 compile --dir (Join-Path $Crate 'res') -o (Join-Path $work 'res.zip')
    if ($LASTEXITCODE -ne 0) { throw 'aapt2 compile failed' }
    $unsigned = Join-Path $work 'unsigned.apk'
    & $aapt2 link -o $unsigned -I $androidJar --manifest (Join-Path $work 'AndroidManifest.xml') (Join-Path $work 'res.zip')
    if ($LASTEXITCODE -ne 0) { throw 'aapt2 link failed' }

    # The app itself: lib/<abi>/libvoxa.so.
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::Open($unsigned, 'Update')
    try {
        foreach ($a in $abis) {
            $so = Join-Path $env:CARGO_TARGET_DIR "$($a.Target)\release\libvoxa.so"
            [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $so, "lib/$($a.Abi)/libvoxa.so", 'Optimal') | Out-Null
        }
    } finally { $zip.Dispose() }

    $aligned = Join-Path $work 'aligned.apk'
    & $zipalign -f -p 4 $unsigned $aligned
    if ($LASTEXITCODE -ne 0) { throw 'zipalign failed' }
    $dist = Join-Path $Root 'dist'
    New-Item -ItemType Directory $dist -Force | Out-Null
    $apk = Join-Path $dist "Voxa-$version.apk"
    & $apksigner sign --ks $env:VOXA_APK_KEYSTORE --ks-pass env:VOXA_APK_KEYPASS --out $apk $aligned
    if ($LASTEXITCODE -ne 0) { throw 'apksigner failed' }
    & $apksigner verify $apk
    if ($LASTEXITCODE -ne 0) { throw 'the APK does not verify' }
    Remove-Item "$apk.idsig" -ErrorAction SilentlyContinue
    Write-Host "Built $apk"
} finally { Pop-Location }
