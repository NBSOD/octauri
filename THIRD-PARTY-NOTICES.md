# 第三方组件声明 / Third-Party Notices

本文件汇总 Octauri 分发物中包含的第三方组件及其许可证要求。
Last updated: 2026-11-16

---

## 1. 随二进制分发的组件

以下组件**实际包含在本项目的发布产物中**（而不只是构建期依赖），
二次分发时必须一并保留其许可证声明。

### 1.1 OpenConnect 9.21（macOS arm64 内置）

| 项 | 内容 |
| --- | --- |
| 位置 | `src-tauri/resources/openconnect/macos-arm64/` |
| 许可证 | **LGPL-2.1**（GNU Lesser General Public License v2.1） |
| 版权 | Copyright © 2009-2025 OpenConnect Contributors |
| 许可证全文 | <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html> |
| 源码获取 | <https://gitlab.com/openconnect/openconnect/-/releases> |
| 项目主页 | <https://www.infradead.org/openconnect/> |

**修改情况**：未修改源代码。为适配随 App 分发，仅对二进制做了
`install_name` 路径重写与 ad-hoc 重新签名（属打包处理，不改变功能逻辑）。

**捆绑的动态库**（同为 LGPL-2.1 或兼容许可，均来自 Homebrew 构建）：
`libopenconnect.5`、`libgnutls.30`、`libnettle.9`、`libhogweed.7`、`libgmp.10`、
`libp11-kit.0`、`libtasn1.6`、`libidn2.0`、`libunistring.5`、`libintl.8`、
`libstoken.1`、`libtomcrypt.1`、`libtommath.1`。

**CA 证书包**：`ca-bundle.pem` 来自 Mozilla CA Certificate Store
（MPL-2.0），仅作为数据文件分发。

### 1.2 vpnc-script

| 项 | 内容 |
| --- | --- |
| 位置 | `src-tauri/resources/openconnect/macos-arm64/vpnc-script` |
| 许可证 | LGPL-2.1（随 OpenConnect 项目分发） |
| 说明 | 该脚本负责写入路由与 DNS，本项目未修改其内容 |

---

## 2. 直接依赖（构建期，随二进制链接）

| 组件 | 版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| [Tauri](https://tauri.app/) | 2.x | MIT OR Apache-2.0 | 应用框架 |
| tauri-plugin-dialog | 2.x | MIT OR Apache-2.0 | 文件选择对话框 |
| [Serde](https://serde.rs/) | 1.x | MIT OR Apache-2.0 | 序列化 |
| serde_json | 1.x | MIT OR Apache-2.0 | JSON 持久化 |
| [keyring](https://github.com/hwchen/keyring-rs) | 3.x | MIT OR Apache-2.0 | 系统钥匙串 |
| libc (unix) | 0.2 | MIT OR Apache-2.0 | 信号与 uid 检测 |
| windows-sys | 0.59 | MIT OR Apache-2.0 | Windows 提权检测 |
| [Vue](https://vuejs.org/) | 3.x | MIT | 前端框架 |
| @tauri-apps/api | 2.x | MIT OR Apache-2.0 | 前端 IPC |

## 3. 传递依赖

Rust 侧共 457 个 crate，许可证分布（`cargo metadata` 实测）：

| 许可证 | 数量 |
| --- | --- |
| MIT OR Apache-2.0 | 216 |
| MIT | 105 |
| Apache-2.0 OR MIT | 38 |
| Unicode-3.0 | 18 |
| Zlib OR Apache-2.0 OR MIT | 17 |
| MIT/Apache-2.0 | 12 |
| Unlicense OR MIT | 10 |
| MPL-2.0 | 7 |
| 其他宽松许可 | 34 |

**不存在 GPL / AGPL 依赖**，全部与本项目的 GPL-3.0 兼容。
MPL-2.0 组件（文件级 copyleft）以未修改的依赖形式使用，不触发其源码公开义务。

完整依赖树与版本锁定见 `src-tauri/Cargo.lock` 与 `package-lock.json`。

---

## 4. 二次分发检查清单

分发本项目的安装包或二进制时，请确认：

- [ ] 附带仓库根目录的 `LICENSE`（GPL-3.0 全文）
- [ ] 附带本文件 `THIRD-PARTY-NOTICES.md`
- [ ] 保留内置 OpenConnect 的 LGPL-2.1 声明与源码获取地址
- [ ] 若修改了 Octauri 源码并对外分发，以 GPL-3.0-or-later 开放源代码
- [ ] 不使用 "Octauri" 名称或图标暗示官方背书（见 README 10.3）