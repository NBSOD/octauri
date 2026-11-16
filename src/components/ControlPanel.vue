<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import {
  HANDSHAKE_WARN_MS,
  connectSelected,
  connStateLabel,
  disconnect,
  forceKill,
  isBusyState,
  refreshEnv,
  selectedProfile,
  state,
} from "../store";

/** 每秒刷新一次，用于连接时长与握手超时提示 */
const now = ref(Date.now());
let timer = 0;
onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
});
onBeforeUnmount(() => window.clearInterval(timer));

const st = computed(() => state.conn.state);
const active = computed(() => isBusyState.value);
const canConnect = computed(
  () => !!selectedProfile.value && !active.value && !state.busy,
);

const title = computed(() => connStateLabel(st.value));

const subtitle = computed(() => {
  const p = selectedProfile.value;
  switch (st.value) {
    case "connected":
      return `已连接：${state.conn.profileName ?? p?.name ?? "-"}`;
    case "handshaking":
      return "正在与网关协商证书与认证…";
    case "disconnecting":
      return "正在释放虚拟网卡与路由…";
    case "error":
      return state.conn.message || "连接异常中断";
    default:
      return p ? `${p.name} · ${p.gateway}` : "请选择或新建一个配置";
  }
});

const elapsedText = computed(() => {
  if (!["connected", "handshaking", "disconnecting"].includes(st.value)) return "";
  const ms = Math.max(0, now.value - state.conn.since);
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
});

const handshakeTimeout = computed(
  () => st.value === "handshaking" && now.value - state.conn.since > HANDSHAKE_WARN_MS,
);

const env = computed(() => state.env);

const authLabel = computed(() => {
  const p = selectedProfile.value;
  if (!p) return "-";
  return p.authMethod === "certificate" ? "客户端证书" : "用户名 / 密码";
});

</script>

<template>
  <section class="col-center">
    <!-- 状态与控制 -->
    <div class="card">
      <div class="status-card">
        <div class="status-indicator" :class="st">
          <Icon :name="st === 'connected' ? 'check' : st === 'error' ? 'alert' : 'shield'" :size="22" />
        </div>
        <div class="status-text">
          <div class="status-title">{{ title }}</div>
          <div class="status-sub">{{ subtitle }}</div>
          <div class="status-pid">
            <template v-if="state.conn.pid">pid {{ state.conn.pid }}</template>
            <template v-if="elapsedText"> · 持续 {{ elapsedText }}</template>
          </div>
        </div>
        <div class="actions">
          <button
            v-if="!active"
            class="btn btn-primary btn-lg"
            :disabled="!canConnect"
            @click="connectSelected()"
          >
            <Icon name="power" :size="16" /> 连接
          </button>
          <button
            v-else
            class="btn btn-danger btn-lg"
            :disabled="st === 'disconnecting'"
            @click="disconnect()"
          >
            <Icon name="x" :size="16" /> 断开
          </button>
          <button
            class="btn"
            :disabled="!state.conn.pid"
            title="立即 SIGKILL / taskkill /F，用于解决进程卡死"
            @click="forceKill()"
          >
            <Icon name="kill" :size="14" /> 强制终止
          </button>
        </div>
      </div>
    </div>

    <!-- 握手超时提示 -->
    <div v-if="handshakeTimeout" class="banner">
      <Icon name="alert" />
      <div>
        握手已持续超过 {{ Math.round(HANDSHAKE_WARN_MS / 1000) }} 秒，仍未建立隧道。
        请检查网关地址、网络连通性，或在日志中确认证书 / 认证是否失败；必要时点击「强制终止」后重试。
      </div>
    </div>

    <!-- 环境告警 -->
    <div v-if="env && env.warnings.length > 0" class="banner error">
      <Icon name="alert" />
      <div>
        <div v-for="(w, i) in env.warnings" :key="i">· {{ w }}</div>
        <pre v-if="!env.elevated" style="margin-top: 6px; color: var(--text-dim)">{{ env.elevationHint }}</pre>
      </div>
    </div>

    <!-- 当前配置 -->
    <div class="card">
      <div class="card-head">
        <span style="flex: 1">当前配置</span>
      </div>
      <div class="card-body">
        <div v-if="!selectedProfile" class="empty" style="padding: 12px 0">
          左侧选择一个配置，或点击 + 新建。
        </div>
        <dl v-else class="kv">
          <dt>名称</dt>
          <dd>{{ selectedProfile.name }}</dd>
          <dt>网关地址</dt>
          <dd>{{ selectedProfile.gateway }}</dd>
          <dt>用户名</dt>
          <dd>{{ selectedProfile.username || "—" }}</dd>
          <dt>认证方式</dt>
          <dd>{{ authLabel }}</dd>
          <dt v-if="selectedProfile.protocol">协议</dt>
          <dd v-if="selectedProfile.protocol">{{ selectedProfile.protocol }}</dd>
          <dt v-if="selectedProfile.certificatePath">客户端证书</dt>
          <dd v-if="selectedProfile.certificatePath">{{ selectedProfile.certificatePath }}</dd>
          <dt v-if="selectedProfile.keyPath">私钥</dt>
          <dd v-if="selectedProfile.keyPath">{{ selectedProfile.keyPath }}</dd>
          <dt>额外参数</dt>
          <dd>{{ selectedProfile.extraArgs || "—" }}</dd>
          <dt>可执行文件</dt>
          <dd>{{ selectedProfile.openconnectPath || "自动探测" }}</dd>
        </dl>
      </div>
    </div>

    <!-- 运行环境 -->
    <div class="card">
      <div class="card-head">
        <span style="flex: 1">运行环境</span>
        <button class="btn btn-ghost" style="height: 24px; text-transform: none" @click="refreshEnv()">
          <Icon name="refresh" :size="13" /> 重新探测
        </button>
      </div>
      <div class="card-body">
        <div v-if="!env" class="hint">正在探测…</div>
        <dl v-else class="kv">
          <dt>平台</dt>
          <dd>{{ env.platform }}</dd>
          <dt>openconnect</dt>
          <dd>
            <span v-if="env.openconnectPath">{{ env.openconnectPath }}</span>
            <span v-else class="faint">未找到</span>
            <span v-if="env.openconnectVersion" class="dim"> — {{ env.openconnectVersion }}</span>
          </dd>
          <dt>权限</dt>
          <dd :style="{ color: env.elevated ? 'var(--success)' : 'var(--error)' }">
            {{ env.elevated ? "已提权（root / 管理员）" : "未提权" }}
          </dd>
          <dt>虚拟网卡</dt>
          <dd :style="{ color: env.tunReady ? 'var(--success)' : 'var(--error)' }">
            {{ env.tunDetail }}
          </dd>
          <template v-if="env.wintunReady !== null">
            <dt>wintun</dt>
            <dd :style="{ color: env.wintunReady ? 'var(--success)' : 'var(--error)' }">
              {{ env.wintunDetail }}
            </dd>
          </template>
          <template v-if="env.candidates.length > 1">
            <dt>候选路径</dt>
            <dd>
              <div v-for="c in env.candidates" :key="c" class="faint">{{ c }}</div>
            </dd>
          </template>
        </dl>
      </div>
    </div>
  </section>
</template>