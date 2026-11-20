# ============================================================================
# Windows + GNU 工具链构建脚本（本机没有 MSVC 时使用）
#
# 用法（在仓库根目录或任意位置）：
#   pwsh -File scripts/build-windows-gnu.ps1
#
# 前置：
#   - rustup 已安装 stable-x86_64-pc-windows-gnu 工具链
#   - MSYS2 的 mingw64 工具链（默认 D:\msys64，可用 OCTAURI_MINGW 覆盖）
#   - npm ci 已执行
#
# 产物：
#   dist\Octauri-portable\Octauri.exe + WebView2Loader.dll + openconnect\
# ============================================================================
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

$mingw = if ($env:OCTAURI_MINGW) { $env:OCTAURI_MINGW } else { 'D:\msys64\mingw64' }
if (-not (Test-Path "$mingw\bin\gcc.exe")) {
    throw "找不到 $mingw\bin\gcc.exe。请安装 MSYS2，或用 OCTAURI_MINGW 指定 mingw64 目录。"
}

$env:PATH = "$env:USERPROFILE\.cargo\bin;$mingw\bin;$env:PATH"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "$mingw\bin\gcc.exe"

Push-Location $root
try {
    Write-Host '[1/3] 构建前端（vue-tsc + vite）'
    npm run build:vite
    if ($LASTEXITCODE -ne 0) { throw 'vite build 失败' }

    Write-Host '[2/3] 编译 Rust（release，GNU 目标）'
    # --features custom-protocol 不能省：否则 Tauri 按 dev 模式编译，
    # exe 启动后会去连 devUrl（localhost:1420）而不是加载内嵌前端。
    cargo +stable-x86_64-pc-windows-gnu build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'cargo build 失败' }

    Write-Host '[3/3] 组装便携目录'
    $dist = Join-Path (Split-Path $root -Parent) 'dist\Octauri-portable'
    Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory "$dist\openconnect" -Force | Out-Null
    Copy-Item 'src-tauri\target\release\octauri.exe' "$dist\Octauri.exe"

    # GNU 构建的 WebView2 是动态加载，必须把 loader 放在 exe 同目录
    $loader = Get-ChildItem "$env:USERPROFILE\.cargo\registry\src" -Recurse -Filter WebView2Loader.dll -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like '*webview2-com-sys*\x64\WebView2Loader.dll' } |
        Select-Object -First 1
    if (-not $loader) { throw '找不到 WebView2Loader.dll（webview2-com-sys 的 x64 版本）' }
    Copy-Item $loader.FullName $dist

    # 只拷文件，避免把 macos-arm64 子目录带进 Windows 包
    Get-ChildItem 'src-tauri\resources\openconnect' -File | Copy-Item -Destination "$dist\openconnect\"
    # 兼容模式需要的 OpenSSL legacy provider（3DES/RC4，给 --allow-insecure-crypto 用）
    if (Test-Path 'src-tauri\resources\openconnect\ossl-modules') {
        Copy-Item 'src-tauri\resources\openconnect\ossl-modules' "$dist\openconnect\" -Recurse
    }

    @'
Octauri Windows 便携版（测试构建）
=================================
1. 双击 Octauri.exe，UAC 点"是"（openconnect 建虚拟网卡需要管理员权限）。
2. 新建配置：填网关地址、用户名；点连接后输入密码。
3. 证书不受信任时，会弹出 pin-sha256 指纹，核对后选"信任"。
4. 数据目录：%APPDATA%\io.github.nbsod.octauri
5. 卸载：删除本目录；wintun 驱动首连时自动注册（会再弹一次 UAC）。
6. 已知限制：不支持 OTP / 短信二次验证。
'@ | Out-File "$dist\README.txt" -Encoding utf8BOM

    Write-Host "完成：$dist"
    Write-Host "运行：双击 Octauri.exe（会弹 UAC）"
} finally {
    Pop-Location
}