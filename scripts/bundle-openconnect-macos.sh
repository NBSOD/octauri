#!/bin/bash
# ============================================================
#  macOS 内置 openconnect 打包脚本
# ============================================================
#  作用：把 Homebrew 装的 openconnect 变成自包含目录
#        （二进制 + 全部第三方 dylib + vpnc-script），
#        以便随 .app 一起分发，用户无需安装 openconnect。
#
#  原理：
#    1. 递归收集 otool -L 里指向 /opt/homebrew 或 /usr/local 的依赖；
#    2. 用 install_name_tool 把绝对路径改写成 @executable_path / @loader_path，
#       —— 必须改写，因为 dyld 对绝对路径的依赖不理会 DYLD_LIBRARY_PATH；
#    3. 改过 Mach-O 的 ad-hoc 签名会失效，需 codesign --force --sign - 重签。
#
#  用法：
#    ./scripts/bundle-openconnect-macos.sh [源二进制] [输出目录]
#
#  默认：
#    源二进制  = which openconnect
#    输出目录  = src-tauri/resources/openconnect/macos-<arch>
#
#  注意：产物架构 = 源二进制架构。Intel 机器上跑本脚本得到 x86_64 版本。
# ============================================================

set -euo pipefail

SRC_BIN="${1:-$(command -v openconnect || true)}"
ARCH="$(uname -m)"
DEFAULT_OUT="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/openconnect/macos-${ARCH}"
OUT_DIR="${2:-$DEFAULT_OUT}"

info() { printf '\033[36m[bundle]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[bundle]\033[0m %s\n' "$*"; }
fail() { printf '\033[31m[bundle]\033[0m %s\n' "$*" >&2; exit 1; }

[ -n "$SRC_BIN" ] || fail "未找到 openconnect。请先 brew install openconnect"
[ -x "$SRC_BIN" ] || fail "不是可执行文件：$SRC_BIN"

# 解析真实路径（Homebrew 的 bin 是符号链接）
SRC_BIN="$(cd "$(dirname "$SRC_BIN")" && pwd)/$(basename "$SRC_BIN")"

info "源二进制：$SRC_BIN"
info "输出目录：$OUT_DIR"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/lib"

cp -L "$SRC_BIN" "$OUT_DIR/openconnect"
chmod u+w "$OUT_DIR/openconnect"

# ---------- 1. 递归收集依赖 ----------
queue=("$OUT_DIR/openconnect")
while [ ${#queue[@]} -gt 0 ]; do
  cur="${queue[0]}"
  queue=("${queue[@]:1}")

  while IFS= read -r dep; do
    case "$dep" in
      /opt/homebrew/* | /usr/local/*)
        base="$(basename "$dep")"
        if [ ! -f "$OUT_DIR/lib/$base" ]; then
          cp -L "$dep" "$OUT_DIR/lib/$base"
          chmod u+w "$OUT_DIR/lib/$base"
          queue+=("$OUT_DIR/lib/$base")
        fi
        ;;
    esac
  done < <(otool -L "$cur" 2>/dev/null | tail -n +2 | awk '{print $1}')
done

info "收集到 $(ls -1 "$OUT_DIR/lib" | wc -l | tr -d ' ') 个依赖库"

# ---------- 2. 重写 install_name ----------
# 主程序：依赖 → @executable_path/lib/<name>
while IFS= read -r dep; do
  case "$dep" in
    /opt/homebrew/* | /usr/local/*)
      install_name_tool -change "$dep" "@executable_path/lib/$(basename "$dep")" \
        "$OUT_DIR/openconnect"
      ;;
  esac
done < <(otool -L "$OUT_DIR/openconnect" | tail -n +2 | awk '{print $1}')

# 库：自身 id 与依赖都用 @loader_path（同目录）
for lib in "$OUT_DIR"/lib/*.dylib; do
  install_name_tool -id "@loader_path/$(basename "$lib")" "$lib" 2>/dev/null || true
  while IFS= read -r dep; do
    case "$dep" in
      /opt/homebrew/* | /usr/local/*)
        install_name_tool -change "$dep" "@loader_path/$(basename "$dep")" "$lib"
        ;;
    esac
  done < <(otool -L "$lib" | tail -n +2 | awk '{print $1}')
done

# ---------- 3. 重新 ad-hoc 签名 ----------
for lib in "$OUT_DIR"/lib/*.dylib; do
  codesign --force --sign - "$lib" >/dev/null 2>&1 || warn "签名失败：$(basename "$lib")"
done
codesign --force --sign - "$OUT_DIR/openconnect" >/dev/null 2>&1 \
  || warn "签名主程序失败（可能影响 Gatekeeper，但通常仍可运行）"

# ---------- 4. vpnc-script 与 CA 证书 ----------
VPNC=""
for cand in \
  "${VPNC_SCRIPT:-}" \
  "$(brew --prefix 2>/dev/null)/etc/vpnc/vpnc-script" \
  "$(brew --prefix 2>/dev/null)/share/vpnc-scripts/vpnc-script" \
  /etc/vpnc/vpnc-script; do
  if [ -n "$cand" ] && [ -f "$cand" ]; then VPNC="$cand"; break; fi
done

if [ -n "$VPNC" ]; then
  cp "$VPNC" "$OUT_DIR/vpnc-script"
  chmod +x "$OUT_DIR/vpnc-script"
  info "vpnc-script：$VPNC"
else
  warn "未找到 vpnc-script，路由与 DNS 不会被配置。可 brew install vpnc-scripts"
fi

# GnuTLS 的默认信任库是编译期写死的 /opt/homebrew/etc/gnutls/cert.pem，
# 目标机器上没有该文件会导致服务器证书校验失败，故随包附带 CA bundle。
CA_BUNDLE=""
for cand in \
  "$(brew --prefix 2>/dev/null)/etc/ca-certificates/cert.pem" \
  "$(brew --prefix 2>/dev/null)/etc/openssl@3/cert.pem" \
  /etc/ssl/cert.pem; do
  if [ -n "$cand" ] && [ -f "$cand" ]; then CA_BUNDLE="$cand"; break; fi
done

if [ -n "$CA_BUNDLE" ]; then
  cp "$CA_BUNDLE" "$OUT_DIR/ca-bundle.pem"
  ca_count="$(grep -c 'BEGIN CERTIFICATE' "$OUT_DIR/ca-bundle.pem" 2>/dev/null || true)"
  info "CA 证书: ${CA_BUNDLE} (${ca_count} 个根证书)"
else
  warn "未找到 CA 证书包，服务器证书验证可能失败"
fi

# ---------- 5. 校验 ----------
info "校验依赖是否仍指向 Homebrew："
if otool -L "$OUT_DIR/openconnect" | grep -qE '/opt/homebrew|/usr/local'; then
  otool -L "$OUT_DIR/openconnect" | grep -E '/opt/homebrew|/usr/local'
  fail "仍有绝对路径依赖，禁止分发"
fi
if ls "$OUT_DIR"/lib/*.dylib 2>/dev/null | head -1 >/dev/null; then
  if otool -L "$OUT_DIR"/lib/*.dylib | grep -qE '/opt/homebrew|/usr/local'; then
    otool -L "$OUT_DIR"/lib/*.dylib | grep -E '/opt/homebrew|/usr/local'
    fail "库中仍有绝对路径依赖，禁止分发"
  fi
fi

info "自包含性验证（加载路径应全部指向产物目录）："
DYLD_PRINT_LIBRARIES=1 "$OUT_DIR/openconnect" --version 2>&1 \
  | grep -oE '(\\@executable_path|\\@loader_path)[^ ]*|/[^ ]*\.dylib' \
  | grep -v '^/usr/lib' | grep -v '^/System' | sort -u | head -15 || true

echo
info "完成。产物：$OUT_DIR"
du -sh "$OUT_DIR" | awk '{print "  体积：" $1}'
"$OUT_DIR/openconnect" --version 2>&1 | head -2