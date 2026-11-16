#!/bin/bash
# ============================================================
#  mock-openconnect —— 本地联调用的假 openconnect
# ============================================================
#  用途：没有真实 VPN 网关时，验证 GUI 的完整链路
#        （子进程拉起 → 参数传递 → 日志流 → 状态机 → 断开 / 强制终止）
#
#  用法：把这个文件的路径填进配置的「openconnect 路径」，然后在
#        「自定义额外参数」里选一个场景：
#
#        --scenario=success     正常连通（默认）
#        --scenario=dtls        DTLS 失败降级，仍连通
#        --scenario=authfail    认证失败，进程以 2 退出
#        --scenario=certfail    证书校验失败，进程以 2 退出
#        --scenario=timeout     永久停在握手阶段（测握手超时提示）
#        --scenario=hang        连通后不响应 SIGTERM（测强制终止）
#
#  它只打印文本，不碰路由、不建网卡、不需要 root。
# ============================================================

set -u

SCENARIO="success"
GATEWAY=""
USER_NAME=""

for arg in "$@"; do
  case "$arg" in
    --version)
      echo "OpenConnect version v9.12-mock (fake, for GUI testing)"
      exit 0
      ;;
    --scenario=*) SCENARIO="${arg#*=}" ;;
    --user=*) USER_NAME="${arg#*=}" ;;
    --passwd-on-stdin|--no-dtls|--verbose|--background) ;;
    --*) ;;
    *) GATEWAY="$arg" ;;
  esac
done

GW="${GATEWAY:-mock.example.com}"

# 消费 GUI 通过 stdin 写入的密码，避免写端收到 EPIPE
if [ ! -t 0 ]; then
  IFS= read -r -t 1 _PASS 2>/dev/null || true
fi

echo "POST https://${GW}/"
sleep 0.4

case "$SCENARIO" in
  timeout)
    echo "Connected to ${GW}:443"
    echo "SSL negotiation with ${GW}"
    # 永远不输出 "Connected as"，用于验证握手超时提醒
    while :; do sleep 1; done
    ;;

  authfail)
    echo "Connected to ${GW}:443"
    echo "SSL negotiation with ${GW}"
    echo "Connected to HTTPS on ${GW}"
    sleep 0.5
    echo "Login failed."
    echo "POST ${GW}/ returned HTTP 401 Unauthorized"
    exit 2
    ;;

  certfail)
    echo "Connected to ${GW}:443"
    echo "SSL negotiation with ${GW}"
    sleep 0.3
    echo "Server certificate verify failed: signer not found"
    echo "Failed to open HTTPS connection to ${GW}"
    exit 2
    ;;

  dtls)
    echo "Connected to ${GW}:443"
    echo "SSL negotiation with ${GW}"
    echo "Connected to HTTPS on ${GW}"
    sleep 0.3
    echo "CSTP connected. DPD 30, Keepalive 20"
    echo "DTLS handshake failed: no DTLS response from peer"
    echo "DTLS is not available, falling back to TLS"
    sleep 0.3
    echo "Connected as 10.20.30.40, using SSL"
    ;;
esac

# 默认 / dtls / hang 场景的公共尾部
case "$SCENARIO" in
  success)
    echo "Connected to ${GW}:443"
    echo "SSL negotiation with ${GW}"
    echo "Connected to HTTPS on ${GW}"
    sleep 0.3
    echo "CSTP connected. DPD 30, Keepalive 20"
    echo "ESP session established with server 203.0.113.7"
    sleep 0.3
    echo "Connected as 10.20.30.40, using SSL"
    echo "add route 10.0.0.0/8 dev tun0"
    echo "add route 172.16.0.0/12 dev tun0"
    echo "Configured as 10.20.30.40, with SSL connected"
    ;;
esac

if [ -n "$USER_NAME" ]; then
  echo "Session authenticated for user '${USER_NAME}'"
fi

# hang 场景故意忽略 SIGTERM，用于验证「强制终止」按钮
if [ "$SCENARIO" = "hang" ]; then
  trap '' TERM
  echo "Running in fake-hang mode, SIGTERM will be ignored by design"
  while :; do sleep 1; done
fi

trap 'echo "Closing session"; echo "disconnect done"; exit 0' TERM INT
while :; do sleep 0.5; done