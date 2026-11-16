此目录用于存放 Windows 平台捆绑的 openconnect 运行时。
This directory holds the bundled openconnect runtime for Windows.

需要放置的文件（需自行获取，本仓库不分发二进制）：
Required files (fetch them yourself; this repo does not ship binaries):

  openconnect.exe      openconnect 主程序（Windows 构建）
  libssl-3-x64.dll     OpenSSL
  libcrypto-3-x64.dll  OpenSSL
  libp11-kit-0.dll     p11-kit（如使用证书认证）
  libgnutls-30.dll     GnuTLS（若使用 GnuTLS 构建）
  libxml2-2.dll        libxml2（若构建依赖）
  vpnc-script-win.js   Windows 下的 vpnc-script（路由/DNS 配置脚本，必需）
  wintun.dll           *不要放在这里* —— 由外层安装脚本部署到程序目录或 System32

要点 / Notes:
1. 打包后这些文件会位于安装目录的 openconnect/ 子目录中，程序启动时
   会优先使用该目录下的 openconnect.exe。
2. vpnc-script-win.js 也需在 PATH 或 openconnect 同目录下，否则 openconnect
   会报 "vpnc-script not found"，无法写入路由与 DNS。
3. macOS 的内置运行时不在本目录，而在同级的 macos-arm64/（自包含：二进制 +
   dylib + vpnc-script + ca-bundle.pem）。Linux 不内置，调用系统已安装的
   openconnect。
   （macOS: Homebrew 路径优先；Linux: 系统 PATH）。
4. 本程序不安装 wintun 驱动，仅做检测与提示。驱动部署由外层 NSIS 脚本完成。

推荐来源 / Where to get:
  https://gitlab.com/openconnect/openconnect/-/releases
  （Windows 构建可使用 MSYS2 或官方提供的安装包解压取得）