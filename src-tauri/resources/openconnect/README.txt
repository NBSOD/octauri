此目录存放随包分发的 openconnect 运行时。
This directory holds the bundled openconnect runtime.

=== 本目录已内置 / Already bundled ===

  wintun.dll           Windows 虚拟网卡驱动库（官方签名版，amd64，v0.14.1）
                       来源：https://www.wintun.net/  请勿改名或修改
  wintun-license.txt   上面的 DLL 附带的 Prebuilt Binaries License
  wintun-README.md     Wintun 官方集成说明

=== 仍需自行放置 / You still need to add ===

  openconnect.exe      openconnect 主程序（Windows 构建）
  libssl-3-x64.dll     OpenSSL
  libcrypto-3-x64.dll  OpenSSL
  libp11-kit-0.dll     p11-kit（如使用证书认证）
  libgnutls-30.dll     GnuTLS（若使用 GnuTLS 构建）
  libxml2-2.dll        libxml2（若构建依赖）
  vpnc-script-win.js   Windows 下的 vpnc-script（路由/DNS 配置脚本，必需）

要点 / Notes:

1. 打包后这些文件会位于安装目录的 openconnect/ 子目录中，程序启动时会优先
   使用该目录下的 openconnect.exe，用户无需安装 openconnect。
2. vpnc-script-win.js 必须在 openconnect.exe 同目录，否则 openconnect 会报
   "vpnc-script not found"，隧道建立后无法写入路由与 DNS。
3. wintun.dll 必须与 openconnect.exe 同目录——这是 Wintun 官方要求的
   side-by-side 部署方式，驱动由 DLL 在首次使用时自动完成注册，
   不需要单独的驱动安装程序，也不需要用户手动操作。
4. 本目录的 wintun.dll 是 amd64（x64）版本。若需支持 Windows on ARM，
   把 wintun.net 压缩包内的 bin/arm64/wintun.dll 替换过来即可（文件名不变）。
5. macOS 的内置运行时在 macos-arm64/（自包含：二进制 + dylib + vpnc-script +
   ca-bundle.pem）。Linux 不内置，调用系统已安装的 openconnect。

推荐来源 / Where to get:
  openconnect: https://gitlab.com/openconnect/openconnect/-/releases
               （Windows 构建可用 MSYS2 或官方安装包解压取得）
  wintun:      https://www.wintun.net/  （必须用官方签名版）