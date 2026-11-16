#!/usr/bin/env bash
# ============================================================================
# 在 Windows 上编译 openconnect 并收集运行所需的全部文件
# ============================================================================
#
# 用法（必须是 MSYS2 的 "MINGW64" 终端，不是 CMD / PowerShell / MSYS）：
#
#   1. 安装 MSYS2：https://www.msys2.org/
#   2. 打开开始菜单里的 "MSYS2 MINGW64"
#   3. cd 到本脚本所在目录，执行：
#        ./scripts/build-openconnect-windows.sh
#
# 产物：./windows-runtime/ 目录，里面是 openconnect.exe、全部依赖 DLL、
# vpnc-script-win.js。把这个目录的内容拷到 src-tauri/resources/openconnect/ 下
# （保留已有的 wintun.dll），提交即可。
#
# 第一次运行会提示缺少依赖包并给出 pacman 命令，装完再跑一次即可。
# ============================================================================

set -euo pipefail

OPENCONNECT_VERSION="${OPENCONNECT_VERSION:-9.21}"
OUT_DIR="${OUT_DIR:-$PWD/windows-runtime}"
SRC_DIR="${SRC_DIR:-$PWD/.build-openconnect}"

info() { printf '\033[36m[build]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[build]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[31m[build]\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------- 环境检查
case "$(uname -s)" in
  MINGW64_NT*|MINGW32_NT*) ;;
  *) die "请在 MSYS2 的 MINGW64 终端中运行（当前：$(uname -s)）" ;;
esac

for t in gcc make pkg-config curl tar; do
  command -v "$t" >/dev/null || die "缺少 $t，请先安装 mingw-w64-x86_64-toolchain"
done

# ---------------------------------------------------------------- 依赖检查
MISSING=()
need_pkg() { pkg-config --exists "$1" 2>/dev/null || MISSING+=("$2"); }

need_pkg openssl        "mingw-w64-x86_64-openssl"
need_pkg libxml-2.0     "mingw-w64-x86_64-libxml2"
need_pkg zlib           "mingw-w64-x86_64-zlib"
need_pkg libpcre2-8     "mingw-w64-x86_64-pcre2"
need_pkg liblz4         "mingw-w64-x86_64-lz4"
need_pkg p11-kit-1      "mingw-w64-x86_64-p11-kit"

if [ ${#MISSING[@]} -gt 0 ]; then
  warn "缺少以下依赖包，请先执行："
  printf '\n  pacman -S --needed \\\n'
  printf '    mingw-w64-x86_64-toolchain \\\n'
  for p in "${MISSING[@]}"; do printf '    %s \\\n' "$p"; done
  printf '    mingw-w64-x86_64-vpnc-scripts \\\n'
  printf '    autoconf automake libtool pkgconf\n\n'
  die "安装后重新运行本脚本"
fi

# vpnc-script：openconnect 靠它配置路由与 DNS，缺了隧道能建但流量不通
VPNC_SCRIPT=""
for cand in /mingw64/etc/vpnc/vpnc-script-win.js /mingw64/share/vpnc-scripts/vpnc-script-win.js; do
  [ -f "$cand" ] && VPNC_SCRIPT="$cand" && break
done
[ -n "$VPNC_SCRIPT" ] || die "找不到 vpnc-script-win.js，请安装 mingw-w64-x86_64-vpnc-scripts"

# ---------------------------------------------------------------- 下载源码
mkdir -p "$SRC_DIR"
cd "$SRC_DIR"
TARBALL="openconnect-${OPENCONNECT_VERSION}.tar.gz"
URL="https://www.infradead.org/openconnect/download/${TARBALL}"

if [ ! -f "$TARBALL" ]; then
  info "下载 ${URL}"
  curl -fL --retry 3 -o "$TARBALL" "$URL"
fi
info "源码 sha256: $(sha256sum "$TARBALL" | cut -d' ' -f1)"
info "官方 PGP 签名: ${URL}.asc（可用 gpg --verify 校验）"

[ -d "openconnect-${OPENCONNECT_VERSION}" ] || tar xf "$TARBALL"
cd "openconnect-${OPENCONNECT_VERSION}"

# ---------------------------------------------------------------- 编译
# 用 OpenSSL 而非 GnuTLS：依赖链更短，收集 DLL 时不易漏，
# 且 Windows 上的根证书可由 Windows 证书存储提供。
if [ ! -f Makefile ]; then
  info "configure"
  ./configure \
    --prefix=/mingw64 \
    --with-openssl \
    --with-vpnc-script="$VPNC_SCRIPT" \
    --disable-nls \
    --without-stoken \
    --without-libpskc \
    --without-libproxy \
    > configure.log 2>&1 || { tail -30 configure.log; die "configure 失败"; }
fi

info "make -j$(nproc)"
make -j"$(nproc)" > build.log 2>&1 || { tail -40 build.log; die "编译失败"; }

[ -f openconnect.exe ] || die "未生成 openconnect.exe"

# ---------------------------------------------------------------- 收集产物
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
cp openconnect.exe "$OUT_DIR/"
cp "$VPNC_SCRIPT" "$OUT_DIR/"

# 递归收集 mingw64 的依赖 DLL（含二级依赖）
info "收集依赖 DLL"
collect() {
  local exe="$1"
  ldd "$exe" 2>/dev/null | awk '{for (i=1;i<=NF;i++) if ($i ~ /^\/mingw64\//) print $i}' || true
}
declare -A SEEN=()
QUEUE=("$OUT_DIR/openconnect.exe")
while [ ${#QUEUE[@]} -gt 0 ]; do
  cur="${QUEUE[0]}"
  QUEUE=("${QUEUE[@]:1}")
  while IFS= read -r dll; do
    [ -n "$dll" ] || continue
    base="$(basename "$dll")"
    [ -n "${SEEN[$base]:-}" ] && continue
    SEEN[$base]=1
    if [ ! -f "$OUT_DIR/$base" ]; then
      cp "$dll" "$OUT_DIR/" 2>/dev/null || continue
      QUEUE+=("$OUT_DIR/$base")
    fi
  done < <(collect "$cur")
done

# ---------------------------------------------------------------- 自检
info "校验产物"
MISSING_DLL=$(ldd "$OUT_DIR/openconnect.exe" 2>/dev/null | grep -c "not found" || true)
if [ "$MISSING_DLL" -gt 0 ]; then
  ldd "$OUT_DIR/openconnect.exe" | grep "not found"
  die "仍有 $MISSING_DLL 个依赖未收集"
fi
"$OUT_DIR/openconnect.exe" --version 2>&1 | head -1 || die "openconnect.exe 无法运行"

# 位数检查：必须与内置的 amd64 wintun.dll 匹配
ARCH="$(file "$OUT_DIR/openconnect.exe" | grep -o 'x86-64\|x86-32\|ARM64' | head -1)"
[ "$ARCH" = "x86-64" ] || warn "openconnect.exe 架构是 ${ARCH:-未知}，仓库内置的 wintun.dll 是 amd64，二者必须一致"

info "完成：$OUT_DIR"
ls -lh "$OUT_DIR" | tail -n +2 | awk '{printf "  %-44s %s\n", $9, $5}'
cat <<'EOF'

后续步骤：
  1. 把上面列出的文件全部拷到
       octauri/src-tauri/resources/openconnect/
     （保留该目录已有的 wintun.dll 与 wintun-license.txt）
  2. 在仓库根目录执行 git add 并提交
EOF