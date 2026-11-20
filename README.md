# Octauri

**Rust + Tauri GUI for the openconnect CLI** · Windows / macOS / Linux

openconnect 命令行的图形外壳（Tauri v2 + Vue 3 + TypeScript），一套源码可编译
Windows / macOS / Linux 三端。

> 本项目与 OpenConnect 官方无任何从属关系，是独立开发的第三方图形外壳。

> 本项目由 AI 编程助手在人类指导下开发，代码经人工审阅、修改与实机验证。
> 详见 [11. 开发方式说明](#11-开发方式说明)。

> ## ⚠️ 项目定位（先读这一段）
>
> **本项目是纯套壳程序。**
>
> - 不含任何 VPN / DTLS / TLS / CSP 协议实现；
> - 不含 TUN/TAP 网卡逻辑，不操作路由表，不修改 DNS；
> - 不含内核驱动，也不安装 wintun 驱动；
> - 全部隧道能力由外部 `openconnect` 可执行文件提供，本程序通过子进程拉起它、
>   传递参数、采集 `stdout`/`stderr`、并在需要时终止它。
>
> 换句话说：**openconnect 能连上的，它就能连；openconnect 连不上的，它也连不上。**

---

## 1. 功能概览

| 模块 | 能力 |
| --- | --- |
| 多配置管理 | 本地 JSON 持久化；新建 / 编辑 / 删除 / 导入 / 导出；每配置独立可执行文件路径与额外参数 |
| 凭据 | **密码不落盘**：只保存网关与账号，每次连接弹窗输入，经 stdin 传给 openconnect |
| 连接控制 | 一键连接 / 断开；优雅终止（SIGTERM）→ 6 秒宽限 → 强制 SIGKILL；独立「强制终止」按钮处理卡死进程 |
| 网关兼容 | Cisco ASA 多 Group：识别 `GROUP: [...]` 提示并弹窗选择，写入 `--authgroup` 后自动重连；也可在配置里预先填写 |
| 状态可视化 | 未连接 → 握手进行中 → 已连通 → 异常断开；含连接时长、pid、握手超时提醒 |
| 实时日志 | 流式输出；证书 / 认证 / 握手 / DTLS / 路由 标签着色；关键字检索与高亮；一键复制全部；导出到文件；前后端双重缓冲上限 |
| 托盘 | 最小化到托盘；右键菜单：连接 / 断开 / 打开窗口 / 退出；左键唤起窗口 |
| 外观 | WinUI 3 / Liquid Glass 风格；深色 / 浅色 / **跟随系统**；半透明毛玻璃面板与原生标题栏主题同步 |
| 跨平台 | Windows / macOS 内置运行时优先，找不到再回退系统安装；Linux 走 PATH；各自检测 wintun / utun / tun |

---

## 2. 目录结构

```
octauri/
├── .github/
│   └── workflows/release.yml    双平台 CI 构建（macOS dmg / Windows nsis）
├── .gitignore
├── .gitattributes               二进制与行尾规则
├── package.json                 前端依赖与脚本
├── vite.config.ts               Vite 配置（固定 1420 端口）
├── index.html
├── scripts/
│   ├── gen-icons.py                 图标生成（Pillow；PNG/ICO/ICNS/托盘）
│   ├── bundle-openconnect-macos.sh  生成 macOS 自包含运行时（重写 install_name + 重签名）
│   ├── mock-openconnect.sh          假 openconnect，无网关时本地联调
│   └── build-openconnect-windows.sh 在 Windows 上编译并收集运行时（MSYS2）
├── src/                         Vue 3 前端
│   ├── main.ts
│   ├── App.vue                  页面切换（连接 / 诊断）/ 对话框编排
│   ├── api.ts                   后端命令薄封装
│   ├── store.ts                 全局状态与业务动作（无 Pinia）
│   ├── types.ts                 与 Rust 侧对齐的类型定义
│   ├── styles.css              设计系统（CSS 变量 + 深浅主题）
│   └── components/
│       ├── TopBar.vue           品牌 / 状态胶囊 / 主题 / 设置 / 关于
│       ├── ProfileList.vue      左栏：配置列表 + 导入导出
│       ├── HomePanel.vue        首页：网关 / 状态 / 隧道 IP / 时长 / 失败摘要
│       ├── DiagPage.vue         诊断页：运行环境 + 完整日志
│       ├── LogPanel.vue         日志流 / 检索 / 复制 / 导出（嵌入诊断页）
│       ├── ProfileDialog.vue    配置编辑表单
│       ├── SettingsDialog.vue   主题 / 全局 openconnect 路径 / 日志上限
│       ├── AboutDialog.vue      LGPL 版权声明
│       ├── PasswordDialog.vue   连接时密码输入
│       ├── AlertDialog.vue      异常弹窗
│       ├── Toasts.vue           轻提示
│       └── Icon.vue             内联 SVG 图标集
└── src-tauri/                   Rust 后端
    ├── Cargo.toml
    ├── build.rs                 嵌入 Windows 管理员清单
    ├── tauri.conf.json
    ├── windows-app-manifest.xml requireAdministrator + PerMonitorV2 DPI
    ├── capabilities/default.json
    ├── icons/                   生成的图标（PNG / ICO / ICNS / 托盘）
    ├── resources/openconnect/
    │   ├── macos-arm64/        内置的 macOS 运行时（自包含，随仓库分发）
    │   └── README.txt          Windows 运行时放置说明
    └── src/
        ├── main.rs
        ├── lib.rs               托盘、窗口行为、命令注册
        ├── commands.rs          Tauri 命令层（唯一后端入口）
        ├── runner.rs            子进程生命周期 / 日志泵 / 信号终止
        ├── probe.rs             跨平台环境探测
        ├── store.rs             JSON 持久化
        ├── single.rs            单实例锁（本机回环端口 + 握手）
        ├── helper.rs            root 辅助进程 + GUI 侧客户端（macOS）
        ├── clipboard.rs         剪贴板写入
        ├── signals.rs           SIGTERM/SIGINT/SIGHUP 转为正常退出（Unix）
        ├── prefs.rs             全局偏好
        └── model.rs             数据模型
```

---

## 3. 环境依赖

### 3.1 通用

| 依赖 | 版本 | 说明 |
| --- | --- | --- |
| Node.js | ≥ 18（实测 22） | 前端构建 |
| Rust | ≥ 1.77（实测 1.100） | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh -s -- -y` |
| openconnect | ≥ 8.0 建议 9.x | **运行必需**，非构建依赖 |

### 3.2 各平台额外要求

**macOS**

```bash
xcode-select --install     # 或安装完整 Xcode
```

macOS **运行时不需要**安装 openconnect——仓库已内置自包含运行时（见 6.1）。
只有需要更新内置版本、或要为目标机器重新打包时才装：

```bash
brew install openconnect && ./scripts/bundle-openconnect-macos.sh
```

**Linux (Debian/Ubuntu)**

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
     pkg-config
sudo apt install openconnect
```

**Windows**

- 安装 [Microsoft C++ 生成工具](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（勾选「使用 C++ 的桌面开发」）
- WebView2 运行时（Win10 1803+ / Win11 通常已内置）
- 自备 `openconnect.exe` 及其依赖 DLL，见 [第 6 节](#6-windows-部署重点)

本项目不依赖任何系统密钥环：密码只在连接时由用户输入，存于进程内存，
不会写入磁盘，也不进入命令行参数。

---

## 4. 开发与编译

```bash
npm install
npm run icons        # 仅在需要重新生成图标时执行（需要 python3 + Pillow）
OCTAURI_NO_ELEVATE=1 npm run dev   # 开发模式（热重载）；macOS 下跳过自动提权，免得每次输密码
npm run build        # 生产构建 + 打包当前平台安装包
npm run typecheck    # 前端类型检查
```

单独构建二进制（不打包）：

```bash
npx tauri build --debug --no-bundle     # 产物：src-tauri/target/debug/octauri
npx tauri build --no-bundle             # 产物：src-tauri/target/release/octauri
```

打包目标在 `src-tauri/tauri.conf.json` 的 `bundle.targets` 中配置，默认
`["nsis", "app", "dmg"]`（Windows 安装包 / macOS 应用与 dmg），实际只会构建当前平台的可用目标。

> **交叉编译提示**：Tauri 不支持跨平台打包安装包。Windows 包须在 Windows 上构建，
> macOS 包须在 macOS 上构建。建议使用 GitHub Actions 或本地虚拟机。

> **Windows 无 MSVC 的替代方案**：如果机器上只装了 MSYS2 的 mingw 工具链，
> 可以执行 `pwsh -File scripts/build-windows-gnu.ps1`，用 GNU 目标一键产出
> `dist/Octauri-portable/`（含 exe、`WebView2Loader.dll`、`openconnect/` 运行时）。
> 注意 GNU 构建的 WebView2 是动态加载，`WebView2Loader.dll` 必须与 exe 同目录。

---

### 本地联调（无需真实网关）

`scripts/mock-openconnect.sh` 是一个假的 openconnect，只打印日志、不建网卡、不要 root，
用于在没有 VPN 环境时验证完整链路：子进程拉起 → 参数传递 → 日志流 → 状态机 → 断开 / 强制终止。

```bash
chmod +x scripts/mock-openconnect.sh

# 在配置里填写：
#   openconnect 路径 = <仓库绝对路径>/scripts/mock-openconnect.sh
#   额外参数         = --scenario=success
```

可用场景（写在配置的「自定义 openconnect 额外参数」里）：

| 场景 | 行为 | 用于验证 |
| --- | --- | --- |
| `--scenario=success` | 正常输出并连通 | 状态机进入「已连通」、日志着色 |
| `--scenario=dtls` | DTLS 失败降级后仍连通 | DTLS 降级告警标签 |
| `--scenario=authfail` | 401 + 退出码 2 | 认证失败弹窗、异常断开状态 |
| `--scenario=certfail` | 证书校验失败 + 退出码 2 | 证书错误标签 |
| `--scenario=timeout` | 停在握手，永不连通 | 60 秒握手超时提醒 |
| `--scenario=hang` | 连通后忽略 SIGTERM | 「强制终止」按钮、6 秒后升级 SIGKILL |

程序检测到可执行文件名含 `mock` 时会跳过提权校验并在日志中注明，生产环境的
`openconnect` 不受影响。

---

### GitHub Actions 自动打包

仓库已包含 `.github/workflows/release.yml`。Tauri 不支持交叉编译打包，因此
CI 用矩阵在 macOS 与 Windows 两个原生 runner 上分别构建（Linux 暂不打包）：

| Runner | 产物 |
| --- | --- |
| `macos-latest`（arm64） | `.dmg`（`.app` 仅作为打包中间产物） |
| `windows-latest` | NSIS 安装包 `.exe` |

**触发方式**

- 推送 tag：`git tag v1.0.0 && git push origin v1.0.0` → 构建并创建**草稿** release，dmg / exe 附在 release 上；
- 手动触发：仓库 Actions 页 → build → Run workflow → 产物在本次运行的 Artifacts 中下载（不创建 release）。
  Artifact 内的文件是**扁平**的（`Octauri_x.y.z_x64-setup.exe` / `Octauri_x.y.z_aarch64.dmg` 直接在压缩包根目录），不会再出现 `release/bundle/nsis/...` 长路径。

**前置条件：Windows 运行时必须入库**

`tauri.conf.json` 的 `bundle.resources` 会把 `src-tauri/resources/openconnect/`
打进安装包的 `openconnect/` 子目录。CI 的 checkout 只能拿到仓库里已有的文件，
所以必须先把以下文件提交进 git（`.gitignore` 已放行）：

```
src-tauri/resources/openconnect/openconnect.exe
src-tauri/resources/openconnect/*.dll          # libssl / libcrypto / libp11-kit 等
src-tauri/resources/openconnect/vpnc-script-win.js   # 必需，否则路由与 DNS 不会写入
```

若不提交，CI 仍能跑通，但打出的 Windows 包内 `openconnect/` 为空，程序启动后
会提示"未找到 openconnect"。workflow 中已加检查步骤，缺失时输出 warning 而不中断构建。

`wintun.dll` **不要**放进这个目录（`.gitignore` 已排除）。按设计它由外层安装脚本
在安装阶段部署，详见 6.3 节。

**macOS 签名与公证（可选）**

workflow 只在配置了 `APPLE_CERTIFICATE` 时才注入签名变量并尝试签名；未配置时完全跳过
（**不能**把空的 `APPLE_CERTIFICATE` 传给 Tauri，否则它会尝试导入空 p12 并报
`Unable to decode the provided data`）。要启用签名，在仓库 Settings → Secrets 中配置：

| Secret | 说明 |
| --- | --- |
| `APPLE_CERTIFICATE` | Developer ID Application 证书的 p12，`base64` 编码后的单行 |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 p12 时设置的密码 |
| `APPLE_SIGNING_IDENTITY` | 如 `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Apple 开发者账号邮箱（公证用） |
| `APPLE_PASSWORD` | App 专用密码，非账号密码 |
| `APPLE_TEAM_ID` | 10 位 Team ID |

未配置签名时，用户首次打开 dmg 内的 app 会被 Gatekeeper 拦截，需右键 →「打开」，
或执行：

```bash
xattr -dr com.apple.quarantine "/Applications/Octauri.app"
```

**成本参考**

public 仓库 Actions 分钟数不限；private 仓库 macOS runner 按 10 倍计费，
本 workflow 一次完整运行约消耗 100 分钟以上额度（macOS 两轮各约占大头）。

**首次推送提醒**

- 确认 `src-tauri/resources/openconnect/` 下的二进制已 `git add`（体积约 10–20 MB）；
- `Cargo.lock` 与 `package-lock.json` 必须入库，否则 CI 无法复现依赖版本；
- `src-tauri/icons/` 已入库，CI 无需执行 `scripts/gen-icons.py`（该脚本需要 python3 + Pillow）。

---

## 5. 权限说明（必读）

`openconnect` 需要 root / 管理员权限才能创建虚拟网卡。

| 平台 | 本程序的处理方式 |
| --- | --- |
| Windows | 通过内嵌清单 `requireAdministrator`，双击即弹 UAC。若便携版被以普通权限启动，界面会显示「未提权」红色告警并阻止连接。 |
| macOS | **GUI 以普通用户运行，openconnect 由 root 辅助进程拉起**。启动时弹一次系统密码框授权该辅助进程，之后连接 / 断开都不再询问。 |
| Linux | 需从终端 `sudo ./octauri` 启动，也可配置 `pkexec` 或 sudoers 免密规则；或给 openconnect 单独 `setcap cap_net_admin+ep`。 |

程序**不会**自行修改 sudoers、不会静默提权、不会写入系统目录。

### 5.1 为什么 macOS 要拆成两个进程

早期版本用 `osascript ... with administrator privileges` 把**整个 GUI** 提权成 root。
这带来一个无法在应用内解决的问题：**macOS 拒绝终止 root 身份的图形程序**。
表现为从 Dock 退出、强制退出、Cmd+Q 之后进程仍在后台，端口一直被占用，
只能 `sudo kill -9` 清理。

现在的分工：

```
Octauri（普通用户）              octauri --helper（root，系统密码框授权）
  ├─ 界面、托盘、配置、日志面板      ├─ 拉起 openconnect
  ├─ 监听 127.0.0.1:<随机端口>  ◄──┤ 连回该端口，转发 stdout/stderr
  └─ 退出 = 断开 socket            └─ 读到 EOF → SIGTERM/SIGKILL openconnect → 退出
```

因为 GUI 不再是 root，Dock 退出、强制退出、注销都能正常结束它；
GUI 无论怎么退出，socket 都会关闭，辅助进程随之清理 openconnect，
所以也不会留下隧道或孤儿进程。

辅助进程只在启动时授权一次，常驻到 GUI 退出为止。授权被拒绝时首页会显示原因。

开发调试可传命令行参数（也可用同名环境变量）：

```bash
Octauri.app/Contents/MacOS/octauri --no-elevate --port 28430
```

| 参数 | 作用 |
| --- | --- |
| `--no-elevate` | 跳过 macOS 自动提权（无法建真实隧道，配合 mock 使用） |
| `--port N` | 覆盖单实例端口（默认 28417），避让冲突 |
| `--spawn PATH` | 启动即用指定可执行文件发起连接（仅 debug 构建） |
| `--exit-after-ms N` | N 毫秒后自动退出（仅 debug 构建） |
| `--terminate-after-ms N` | N 毫秒后调用 `[NSApp terminate:]`，验证退出接管（仅 debug 构建） |

环境变量 `OCTAURI_NO_ELEVATE=1` 同样可跳过自动提权（此时无法建立真实隧道，
配合 `scripts/mock-openconnect.sh` 使用）。

macOS 上的配置文件由 GUI（普通用户）写入 `~/Library/Application Support/`，
不会落到 `/var/root`。若你手动用 `sudo` 启动 GUI，程序会通过
`stat -f %Su /dev/console` 定位真实登录用户并改写路径，避免配置“消失”。

### 退出行为

退出 Octauri 时都会**先结束 openconnect**：发 SIGTERM 让它调用
vpnc-script 还原路由与 DNS，超时未退出再 SIGKILL，然后进程结束。

macOS 上这一步由 root 辅助进程完成（GUI 无权向 root 进程发信号）：
GUI 发出 `QUIT` 或直接退出（socket 关闭），辅助进程收到后清理 openconnect 再退出。

覆盖的退出入口：

| 入口 | 实现 |
| --- | --- |
| 托盘「退出」 | `AppHandle::exit` |
| Cmd+Q / 菜单「退出 Octauri」 | 接管 `-[NSApplication terminate:]` |
| Dock 图标「退出」/ 系统关机 | 接管 delegate 的 `applicationShouldTerminate:` |
| 注销、`kill`、Ctrl+C | signals：SIGTERM / SIGINT / SIGHUP |

为什么需要接管系统退出：AppKit 的终止流程会尝试关闭所有窗口，而「关窗口收起到托盘」
调用了 prevent_close，窗口不关闭 → 终止被取消 → 进程留在后台，端口也一直占着。
接管后一律走 `AppHandle::exit`，即托盘「退出」那条路径。

不这么做的话，openconnect 会以 root 孤儿进程残留，隧道与路由都还在，只能
`sudo kill -9` 清理。清理完成后程序直接结束进程——macOS 上等待子进程会占用主线程，
事件循环的自然收尾可能因此卡住，表现为端口已释放但 GUI 进程仍在。

**剪贴板**：早期 GUI 以 root 运行时，剪贴板按用户会话隔离导致复制失效；
现在 GUI 是普通用户，复制由后端直接调用 `pbcopy` 即可。代码里保留了
`launchctl asuser` 分支，仅用于手动 `sudo` 启动的兜底场景。

---

## 6. 内置运行时与前置部署

程序按 `内置 → 全局设置 → 自动探测` 的优先级选择 openconnect。
Windows 与 macOS 随包内置运行时，Linux 依赖发行版包管理器。

### 6.1 macOS：已内置 openconnect

`src-tauri/resources/openconnect/macos-arm64/` 是一份自包含的 openconnect 9.21：

```
macos-arm64/
├── openconnect          主程序（94 KB）
├── lib/*.dylib          13 个第三方库，install_name 已重写为 @executable_path
├── vpnc-script          路由与 DNS 配置脚本
└── ca-bundle.pem        184 个根证书（GnuTLS 默认信任库路径编译期写死，必须随包带）
```

三个必须知道的处理：

1. **install_name 必须重写**。dyld 对绝对路径依赖不理会 `DYLD_LIBRARY_PATH`，
   因此脚本用 `install_name_tool` 把 `/opt/homebrew/...` 全部改成
   `@executable_path/lib/...`，否则换机器必然启动失败。
2. **CA 证书必须随包带**。GnuTLS 把信任库路径编译进二进制
   （`/opt/homebrew/etc/gnutls/cert.pem`），干净机器上不存在就会导致
   `certificate verify failed`。程序检测到同目录有 `ca-bundle.pem` 时自动追加
   `--cafile=`，用户仍可用额外参数覆盖。
3. **vpnc-script 要绕开空格路径**。openconnect 以 shell 拼接命令执行该脚本，
   而 `.app` 名字通常含空格（`Octauri.app`）。程序检测到路径含空格时
   会先复制到 `$TMPDIR/octauri-scripts/` 再传入。

重新生成（openconnect 升级后执行）：

```bash
brew install openconnect
./scripts/bundle-openconnect-macos.sh
```

脚本会递归收集 `otool -L` 里的依赖、重写路径、ad-hoc 重签名，并校验产物中
不再残留 `/opt/homebrew` 绝对路径（校验不通过直接失败，避免发出坏包）。

**架构限制**：仅支持 Apple Silicon（arm64）。本项目是个人向工具，暂不覆盖 Intel Mac；
在 x86_64 目标上编译时程序会自动回退到系统安装的 openconnect。

**功能限制**：内置版不含 p11-kit 的 PKCS#11 trust 模块，智能卡 / USB Token
认证不可用（`p11-kit-trust.dylib` 是运行时 `dlopen` 加载，不在链接依赖里）。
用户名密码认证不受影响。

### 6.2 Windows：wintun 已内置，只需补 openconnect

`src-tauri/resources/openconnect/wintun.dll` 已经随仓库分发
（**Wintun 0.14.1 官方签名版，amd64**，SHA256 与 wintun.net 公布值一致）。

**驱动不需要单独安装。** Wintun 的设计是把签名驱动内嵌在 DLL 里，首次被
openconnect 调用时通过 SetupAPI 自动完成注册——用户不需要下载驱动、不需要
运行安装程序、不需要手动配置，只会在第一次连接时看到一次 UAC 提权弹窗。

openconnect 本体需要自行编译后放入同一目录。**官方不提供 Windows 预编译
二进制**（GitLab releases 只有源码包，MSYS2 仓库也没有该包），因此仓库提供了
一键编译脚本：

```bash
# 在 MSYS2 的 "MINGW64" 终端中，于仓库根目录执行
./scripts/build-openconnect-windows.sh
```

脚本会下载指定版本的源码（默认 9.21，可用 `OPENCONNECT_VERSION` 覆盖）、
用 OpenSSL 后端编译、递归收集全部依赖 DLL、校验无缺失依赖，并检查产物是
x86-64（必须与内置的 amd64 `wintun.dll` 一致）。

首次运行会列出缺少的 pacman 包，装完再跑一次即可。产物 `windows-runtime/`
里的文件全部拷入 `src-tauri/resources/openconnect/` 后提交。

其中 `vpnc-script-win.js` **必需**（openconnect 靠它配置路由与 DNS，缺失时会报
`vpnc-script not found`，隧道虽建立但流量不通）。MSYS2 仓库中并没有
`mingw-w64-x86_64-vpnc-scripts` 这个包，脚本会从 vpnc-scripts 上游自动下载。

> 历史说明：`openconnect-gui` 项目发布过 2019 年的 `win32` 安装包，内含旧版
> openconnect。**不要使用**：它是 32 位（无法加载 amd64 的 `wintun.dll`），
> 且版本停留在 8.x 时代，与现代网关的兼容性没有保证。

`wintun.dll` 无需处理。若要支持 Windows on ARM，用 wintun.net 压缩包里的
`bin/arm64/wintun.dll` 替换同名文件即可。

**合规提示**（Wintun Prebuilt Binaries License）：必须使用 wintun.net 的官方
签名 DLL、保持原文件名 `wintun.dll`、不得反编译或修改、不得移除版权声明、
不得以 WireGuard / Wintun 名义为本项目背书。该许可证明确允许随通过其 API
使用它的软件一起分发，因此与 GPL-3.0 的 Octauri 分发不冲突。

### 6.3 放置位置

```
安装目录/
├── Octauri.exe
└── openconnect/
    ├── openconnect.exe
    ├── vpnc-script-win.js     ← 必需
    ├── wintun.dll             ← 已内置，随包分发
    ├── wintun-license.txt
    ├── wintun-README.md
    └── *.dll
```

`src-tauri/resources/openconnect/` 中的内容会被整体打包到安装目录的
`openconnect/` 子目录下，程序启动时优先使用该路径。

`wintun.dll` 必须与 `openconnect.exe` 同目录——这是 Wintun 要求的 side-by-side
部署方式。程序的检测顺序为：`openconnect.exe` 同目录 → 主程序 exe 同目录 →
`%SystemRoot%\System32`，任一位置命中即视为就绪。

### 6.4 两类交付形态

**A. 便携免安装版**（U 盘直接运行；仅本地构建，CI 不产出）

```bash
npx tauri build --no-bundle
```

将 `src-tauri/target/release/octauri.exe` 与 `src-tauri/resources/openconnect/`
整个目录拷到同一位置即可，其中已含 wintun.dll，无需额外部署。不写注册表。

**B. NSIS 安装包**

```bash
npx tauri build --bundles nsis
```

安装包会把 `resources/openconnect/`（含 wintun.dll）部署到安装目录，
装完即可连接，无需任何后续手动步骤。

> `src-tauri/nsis/wintun-hook.nsh.example` 是早期设计留下的自定义 hook 示例，
> 在 wintun 已随包内置后**不再需要**。保留它仅供你添加其他安装期自定义步骤时参考。

---

## 7. 配置与数据存放位置

| 内容 | 路径 |
| --- | --- |
| 配置列表 | `{appDataDir}/profiles.json`（macOS `~/Library/Application Support/io.github.nbsod.octauri/`，Windows `%APPDATA%\io.github.nbsod.octauri\`，Linux `~/.config/io.github.nbsod.octauri/`） |
| 全局设置 | 同目录 `prefs.json` |
| 密码 | **不存储**。每次连接时输入，仅存在于进程内存 |
| 日志 | 仅内存（后端 2000 条环形缓冲 + 前端可配上限），不落盘；需要留存请手动导出 |

`profiles.json` 采用「写临时文件 + rename」的原子写入，文件权限在类 Unix 系统上
设为 `0600`。解析失败时会备份为 `profiles.corrupt-<时间戳>.json` 而不是直接清空。

导入配置时**不会**导入密码，且强制关闭 `rememberPassword`，避免凭据随文件扩散。

---

## 8. 排障对照表

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| 「未找到 openconnect 可执行文件」 | 未安装或不在探测路径 | 安装 openconnect，或在设置 / 配置中手动指定路径 |
| 启动即报 `Permission denied` | 缺少 root / 管理员权限 | Windows 右键「以管理员身份运行」；macOS/Linux 用 `sudo` 启动 |
| 连接成功但无法访问内网 | `vpnc-script` 缺失，路由未写入 | 部署 `vpnc-script-win.js`（Windows）或 `vpnc-script`（类 Unix），并确保其在 PATH 中 |
| Windows 提示 wintun 未就绪 | 未部署 `wintun.dll` | 按第 6 节放置 DLL 后重新点「重新探测」 |
| 日志出现 `DTLS ... failed` / 降级为 TLS | UDP 被防火墙拦截 | 属正常降级，连接仍可用但吞吐下降；可在额外参数加 `--no-dtls` 消除告警 |
| 日志出现 `certificate verify failed` | 服务端证书不被信任（自签 / 中间证书缺失 / 时钟偏差） | 校时、补全 CA，或谨慎使用 `--servercert pin-sha256:...` |
| 日志出现 `Login failed` / `401` | 账号密码错误或认证方式不匹配 | 核对凭据；证书模式勿勾选密码 |
| 日志出现 `GROUP: [a\|b]:` 后认证失败 | Cisco ASA 多组网关要求先选 Group | 连接时会弹窗选择；也可在配置里填写 Group（`--authgroup`） |
| 日志出现 `SSL alert number 40` / `handshake failure` | 老 ASA 只接受静态 RSA 等旧套件（如 `AES128-SHA`），OpenSSL 3 默认安全级别禁用 | 配置里勾选「兼容模式：TLS 1.2 + 旧加密套件」后重试 |
| 进程不退、端口占用 | openconnect 卡在 teardown | 点「强制终止」（SIGKILL / `taskkill /F /T`） |
| 退出 Octauri 后仍有 root 进程残留 | 1.0 早期版本退出时未结束 openconnect 子进程 | 已修复：任何退出路径（托盘、Cmd+Q、SIGTERM/SIGINT/SIGHUP）都会先 SIGTERM openconnect，3 秒未退再 SIGKILL。若仍残留可 `sudo pkill -f Octauri.app` |
| 日志出现 `** Error: The parameters were not valid.` | 原版 vpnc-script 在全隧道下查不到网络服务名（macOS） | 内置脚本已修复。若用自定义 openconnect，DNS 实际已由 scutil 配好，该报错可忽略 |
| macOS 关闭窗口后程序仍在 | 这是设计行为 | 托盘右键「退出」或 Cmd+Q 退出 |
| 保存配置报 `Permission denied (os error 13)` | 配置目录属主是 root（旧版以 root 运行的遗留），普通用户无权写入 | 程序会在首页提示修复命令，执行一次即可：`sudo chown -R "$(id -un)" "$HOME/Library/Application Support/io.github.nbsod.octauri"` |
| macOS 首页提示「无法建立管理员授权」 | 启动时取消了系统密码框 | 退出 Octauri 重新打开，在弹窗中允许；辅助进程只在启动时授权一次 |
| macOS 退出后仍残留 `octauri --helper` | 1.0 早期版本的 GUI 本身是 root，系统无法终止 | 已改为「普通 GUI + root 辅助进程」。GUI 退出（含强制退出）会关闭 socket，辅助进程随之清理 openconnect 并退出 |

---

## 9. 已知限制

1. **不解析 openconnect 的交互式提示**。遇到需要二次输入（如 OTP、SMS 验证码）的
   网关，当前版本无法完成交互，会表现为握手超时。可通过额外参数
   `--token-mode=...` 或改用证书认证规避。
2. **密码模式仅支持 `--passwd-on-stdin` 一次性投喂**，不处理多轮质询。
3. **不提供流量统计、分应用分流、自动重连**。这些均属协议层/系统层能力，超出外壳定位。
4. **单实例**：通过本机回环端口 `127.0.0.1:28417` 实现，重复启动会唤起已有窗口后退出。
   - macOS：检测在弹密码框之前，已运行时再次双击**不会**再弹密码框
   - Windows：`requireAdministrator` 清单使 UAC 弹窗先于任何代码执行，
     因此重复双击仍会弹一次 UAC，通过后唤起已有窗口并退出，不会出现两个实例
   - 若该端口被其他程序占用，程序照常启动，只是本次运行不启用单实例保护；
     可用环境变量 `OCTAURI_PORT` 指定其他端口
5. **macOS 依赖 root 辅助进程**：启动时的授权被拒绝后，本次运行无法建立连接，
   需要退出重开。辅助进程常驻到 GUI 退出为止，不做进程间复用。
6. **Linux 不捆绑 openconnect**，依赖发行版包管理器安装。Windows 与 macOS（arm64）
   已内置，见第 6 节。

---

## 10. 开源许可与合规

### 10.1 本项目

**Octauri** 以 **GNU 通用公共许可证第 3 版或更新版本（GPL-3.0-or-later）** 发布，
Copyright © 2025 Octauri Contributors。全文见仓库根目录的 [`LICENSE`](./LICENSE)。

选择 GPL 而非 MIT 的原因：本项目允许任何人使用与商业使用，但**不允许把衍生版本
闭源分发**。若有人基于 Octauri 修改后闭源售卖，作者有权要求其开放源代码。

需要清楚的边界：**GPL 允许商业使用**，它限制的是「闭源分发衍生作品」，不是「收费」。
如果目标是完全禁止商用，GPL 做不到，需要改用非 OSI 开源许可（代价是无法标为开源项目）。

### 10.2 内置的 OpenConnect

`src-tauri/resources/openconnect/macos-arm64/` 随仓库分发一份 OpenConnect 9.21
（macOS arm64 构建），其许可证为 **LGPL-2.1**：

- Copyright © 2009-2025 OpenConnect Contributors
- 许可证全文：<https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>
- 源码获取：<https://gitlab.com/openconnect/openconnect/-/releases>
- 项目主页：<https://www.infradead.org/openconnect/>

**兼容性说明**：LGPL-2.1 允许被 GPL-3.0 项目包含与再分发，二者不冲突。
本项目未修改 OpenConnect 源码，仅以独立进程方式调用其可执行文件，
不构成衍生作品——即使不采用 GPL，本项目也有权选择任意许可证。

**分发义务**：二次分发本仓库或安装包时，须保留 `LICENSE`、
`THIRD-PARTY-NOTICES.md`，并提供上表中的源码获取途径。

### 10.3 商标

**Octauri 的名称与图标不在 GPL 授权范围内**，项目保留全部权利。

GPL 明确不授予商标权。这意味着：

- 他人可以自由使用、修改、分发代码；
- 但不得使用 "Octauri" 名称或本项目图标暗示官方背书、或以本项目名义发布衍生版；
- 修改版请更换名称与图标。

这是 GPL 之外唯一的额外限制，也是防止「拿代码改名去卖还声称是原版」的实际手段——
代码许可证管不了品牌，商标才可以。

### 10.4 第三方依赖

本项目使用的 Rust 与前端依赖全部为宽松许可（MIT / Apache-2.0 为主），
与 GPL-3.0 兼容。完整清单与主要组件声明见 [`THIRD-PARTY-NOTICES.md`](./THIRD-PARTY-NOTICES.md)。

---

## 11. 开发方式说明

**本项目是 AI 辅助开发（vibe coding）的产物，这一点如实披露。**

分工大致如下：

| 环节 | 由谁完成 |
| --- | --- |
| 需求定义与硬性约束（纯套壳、禁止自研协议栈、依赖控制、平台适配要求） | 人类提出，并逐条校验实现是否越界 |
| 技术选型与架构决策（子进程模型、状态机、许可证选择、内置运行时方案） | 人类拍板，AI 提供方案与取舍分析 |
| 代码编写、重构、文档撰写 | AI 生成 |
| 代码审阅、逐轮修改方向、界面验收 | 人类 |
| 编译、类型检查、单元测试、界面截图核验 | AI 执行，人类确认结果 |
| 真实环境联调 | 尚未完成，见第 9 节「已知限制」 |

### 这意味着什么

**不要默认它已经过充分检验。** 当前的验证强度是：`cargo check` / `cargo test`（2 个单元测试）/
`vue-tsc` / `vite build` 全部通过，界面在深色与浅色主题下逐屏核验，
macOS 内置运行时经 `otool` 校验无外部路径依赖。**但没有在真实 VPN 网关上跑过一次完整连接**。

**代码可能包含不自然的写法。** AI 生成代码常见的模式是「逻辑正确但结构冗余」，
本项目已按极简原则做过一轮收敛（Rust 侧 8 个源文件、前端无 UI 库与状态管理库），
但仍有改进空间。

**若用于生产环境，请自行审查**，尤其是进程管理（`runner.rs`）与凭据处理（`store.rs`）两处。

### 关于版权

GPL-3.0 的效力以版权存在为前提。本项目的人类贡献体现在：需求与约束的定义、
架构与许可决策、每一轮改动的审阅与取舍、验证标准的确立。这些构成对作品整体的
创造性控制。但不宜声称「每一行代码均由人类独立编写」。

如果你要基于本项目做商业分发或法律主张，建议先做独立的法律评估。

### 这是缺陷吗

不是。AI 辅助开发把实现周期从数周压缩到数小时，
代价是必须自己承担验证责任。本项目把「已知未验证项」明确写在
[第 9 节](#9-已知限制)，而不是假装它已经全部就绪。
