<#
.SYNOPSIS
    Build OpenPhira end to end: Rust engine -> encrypted assets -> APK.

.DESCRIPTION
    Three stages, each independently skippable:

      1. Rust    cross-compile `phira` (cdylib) for arm64-v8a with the NDK's clang
      2. Assets  encrypt `res/**` with the 8-byte cipher the engine decrypts at startup
      3. APK     Gradle assembles the Java shell with the .so and the assets baked in

    Nothing here needs Android Studio; it needs a JDK 21, the Android SDK (build-tools,
    platform-33, NDK) and a Rust toolchain with the `aarch64-linux-android` target.

.EXAMPLE
    ./build.ps1 -Release
#>
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$SkipRust,
    [switch]$SkipAssets,
    [switch]$SkipGradle,
    [switch]$SkipSign,
    [string]$AndroidHome = $(if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { 'C:\Android\sdk' }),
    [string]$NdkVersion = '27.0.12077973',
    [string]$JdkPath = $env:JAVA_HOME,
    [int]$MinSdk = 22
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root    = $PSScriptRoot          # this directory *is* the cargo workspace
$android = Join-Path $root 'android'
$abi     = 'arm64-v8a'
$soName  = 'libphira.so'

function Step($text) { Write-Host "`n=== $text ===" -ForegroundColor Cyan }
function Need($path, $what) {
    if (-not (Test-Path $path)) { throw "$what not found: $path" }
}

# ---------------------------------------------------------------- toolchain discovery
$ndk       = Join-Path $AndroidHome "ndk\$NdkVersion"
$llvmBin   = Join-Path $ndk 'toolchains\llvm\prebuilt\windows-x86_64\bin'
$clang     = Join-Path $llvmBin "aarch64-linux-android$MinSdk-clang.cmd"
$clangxx   = Join-Path $llvmBin "aarch64-linux-android$MinSdk-clang++.cmd"
$llvmAr    = Join-Path $llvmBin 'llvm-ar.exe'

# A JDK 21 is required: Gradle 8.7 refuses to run on anything newer.
$jdk = $JdkPath
if (-not $jdk) {
    # A JDK dropped next to the project is used automatically.
    foreach ($candidate in @((Join-Path $root '_toolchain\jdk21'), (Join-Path $root '..\_toolchain\jdk21'))) {
        if (Test-Path $candidate) {
            $jdk = (Get-ChildItem $candidate -Directory | Select-Object -First 1).FullName
            break
        }
    }
}

$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }

# ---------------------------------------------------------------- 1. Rust engine
if (-not $SkipRust) {
    Step 'Rust: cross-compiling libphira.so'
    Need $clang 'NDK clang'

    # cargo passes these to rustc/cc; without them the `cc` crate invokes the host compiler.
    $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
    $env:CC_aarch64_linux_android   = $clang
    $env:CXX_aarch64_linux_android  = $clangxx
    $env:AR_aarch64_linux_android   = $llvmAr
    $env:ANDROID_HOME = $AndroidHome
    $env:NDK_HOME     = $ndk

    # Built as an explicit array: `$x = if (...) { @('--release') }` collapses to a bare string
    # and `@x` then splats it character by character.
    $cargoArgs = @('build', '-p', 'phira', '--lib', '--no-default-features', '--target', 'aarch64-linux-android')
    if ($Release) { $cargoArgs += '--release' }

    Push-Location $root
    try {
        & $cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    } finally { Pop-Location }

    $profileDir = if ($Release) { 'release' } else { 'debug' }
    $built = Join-Path $root "target\aarch64-linux-android\$profileDir\$soName"
    Need $built 'built shared library'

    $jniLibs = Join-Path $android "app\src\main\jniLibs\$abi"
    New-Item -ItemType Directory -Force -Path $jniLibs | Out-Null
    Copy-Item $built (Join-Path $jniLibs $soName) -Force

    # libphira.so links the NDK's shared C++ runtime, so it has to travel with it.
    $libcxx = Join-Path $llvmBin '..\sysroot\usr\lib\aarch64-linux-android\libc++_shared.so'
    Need $libcxx 'libc++_shared.so'
    Copy-Item $libcxx (Join-Path $jniLibs 'libc++_shared.so') -Force

    $sizeMb = [math]::Round((Get-Item $built).Length / 1MB, 1)
    Write-Host "  $soName -> jniLibs/$abi ($sizeMb MB)"
    Write-Host "  libc++_shared.so -> jniLibs/$abi"
}

# ---------------------------------------------------------------- 2. assets
if (-not $SkipAssets) {
    Step 'Assets: encrypting res/**'
    $srcAssets = Join-Path $root 'assets'
    $dstAssets = Join-Path $android 'app\src\main\assets'
    Need $srcAssets 'asset source'

    if (Test-Path $dstAssets) { Remove-Item $dstAssets -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $dstAssets | Out-Null

    $py = (Get-Command python -ErrorAction SilentlyContinue).Source
    if (-not $py) { throw 'python not found' }
    & $py (Join-Path $root 'scripts\encrypt_assets.py') $srcAssets $dstAssets --verify
    if ($LASTEXITCODE -ne 0) { throw 'asset encryption failed' }
}

# ---------------------------------------------------------------- 3. APK
if (-not $SkipGradle) {
    Step 'APK: Gradle assemble'
    Need (Join-Path $android 'gradlew.bat') 'Gradle wrapper'
    if (-not $jdk) { throw 'JAVA_HOME is not set and no bundled JDK was found' }
    $env:JAVA_HOME = $jdk
    $env:ANDROID_HOME = $AndroidHome

    # Forward slashes keep the .properties file valid without escaping.
    $sdkDir = $AndroidHome -replace '\\', '/'
    Set-Content -Path (Join-Path $android 'local.properties') -Value "sdk.dir=$sdkDir" -Encoding ASCII

    $task = if ($Release) { 'assembleRelease' } else { 'assembleDebug' }
    Push-Location $android
    try {
        & .\gradlew.bat --no-daemon $task
        if ($LASTEXITCODE -ne 0) { throw "gradlew $task failed" }
    } finally { Pop-Location }

    $apkDir = Join-Path $android "app\build\outputs\apk"
    $apk = Get-ChildItem $apkDir -Recurse -Filter *.apk |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $apk) { throw "no APK under $apkDir" }
    Write-Host "`nAPK: $($apk.FullName) ($([math]::Round($apk.Length / 1MB, 1)) MB)" -ForegroundColor Green

    # ------------------------------------------------------------ 4. sign
    if (-not $SkipSign) {
        Step 'Sign: apksigner'
        $buildTools = Join-Path $AndroidHome 'build-tools\34.0.0'
        $apksigner  = Join-Path $buildTools 'apksigner.bat'
        $keytool    = Join-Path $jdk 'bin\keytool.exe'
        Need $apksigner 'apksigner'
        Need $keytool 'keytool'

        # A throwaway self-signed key is enough for sideloading. Replace it with your own to
        # upgrade an existing install.
        $keystore = Join-Path $root 'keystore.jks'
        if (-not (Test-Path $keystore)) {
            Write-Host '  generating keystore.jks (storepass/keypass: android)'
            & $keytool -genkeypair -keystore $keystore -alias openphira -keyalg RSA `
                -keysize 2048 -validity 10000 -storepass android -keypass android `
                -dname 'CN=OpenPhira' 2>&1 | Out-Null
        }

        $signed = Join-Path $apk.DirectoryName ($apk.BaseName -replace '-unsigned$', '-signed.apk')
        & $apksigner sign --ks $keystore --ks-pass pass:android --key-pass pass:android `
            --out $signed $apk.FullName
        if ($LASTEXITCODE -ne 0) { throw 'apksigner sign failed' }

        & $apksigner verify --print-certs $signed | Select-Object -First 3
        Write-Host "SIGNED: $signed" -ForegroundColor Green
    }
}
