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
  selectedProfile,
  state,
} from "../store";

/**
 * 辅助进程提示：授权失败时说明原因并提示重启；
 * 授权进行中时提示用户在弹出的系统窗口里输入密码。
 */
/** 配置目录不可写：多为旧版本以 root 运行时留下的属主问题 */
const dataDirNotice = computed(() => {
  const issue = state.dataDirIssue;
  if (!issue) return null;
  return `配置目录不可写（${issue.path}），保存会失败。请在终端执行一次后重启本程序：${issue.fix}`;
});

const helperNotice = computed(() => {
  if (state.helper.state === "error") {
    return {
      level: "error",
      text: `无法建立管理员授权：${state.helper.error}。退出后重新打开 Octauri，并在系统弹窗中允许。`,
    };
  }
  if (state.helper.state === "starting" && state.info?.platform === "macos") {
    return { level: "warn", text: "正在请求管理员授权，请在系统弹窗中输入密码…" };
  }
  return null;
});


/** 每秒刷新一次，用于连接时长与握手超时提示 */
const now = ref(Date.now());
let timer = 0;
onMounted(() => {
  timer = window.setInterval(() => (now.value = Date.now()), 1000);
});
onBeforeUnmount(() => window.clearInterval(timer));

const st = computed(() => state.conn.state);
const active = computed(() => isBusyState.value);
const canConnect = computed(() => !!selectedProfile.value && !active.value && !state.busy);

/** 当前连接对应的配置；未连接时显示选中的配置 */
const isCurrent = computed(
  () => !!selectedProfile.value && state.conn.profileId === selectedProfile.value.id,
);
const showLive = computed(() => isCurrent.value && st.value !== "idle");

const gateway = computed(() => {
  if (showLive.value && state.conn.gateway) return state.conn.gateway;
  return selectedProfile.value?.gateway ?? "";
});

const elapsed = computed(() => {
  if (!showLive.value || !["connected", "handshaking", "disconnecting"].includes(st.value))
    return "";
  const s = Math.floor(Math.max(0, now.value - state.conn.since) / 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
});

const handshakeSlow = computed(
  () => showLive.value && st.value === "handshaking" && now.value - state.conn.since > HANDSHAKE_WARN_MS,
);

/** 失败摘要：优先最近一条带故障标签的错误，其次状态消息 */
const errorSummary = computed(() => {
  if (!showLive.value || st.value !== "error") return "";
  for (let i = state.logs.length - 1; i >= 0; i--) {
    const l = state.logs[i];
    if (l.level === "error" && l.stream !== "app") return l.text;
  }
  return state.conn.message || "连接异常断开";
});

const envBlocking = computed(() => (state.env?.warnings.length ?? 0) > 0);

const statusClass = computed(() => (showLive.value ? st.value : "idle"));
const statusText = computed(() => connStateLabel(statusClass.value as typeof st.value));
const icon = computed(() =>
  statusClass.value === "connected" ? "check" : statusClass.value === "error" ? "alert" : "shield",
);

function goDiag() {
  state.page = "diag";
}
</script>

<template>
  <section class="home">
    <!-- macOS：GUI 以普通用户运行，openconnect 由 root 辅助进程拉起。
         用户取消授权或辅助进程未就绪时，这里给出明确原因与处置办法。 -->
    <div v-if="dataDirNotice" class="home-banner error">
      <Icon name="shield" :size="14" />
      <span>{{ dataDirNotice }}</span>
    </div>
    <div v-else-if="helperNotice" class="home-banner" :class="helperNotice.level">
      <Icon name="shield" :size="14" />
      <span>{{ helperNotice.text }}</span>
    </div>

    <div v-if="!selectedProfile" class="home-empty">
      <Icon name="shield" :size="40" class="faint" />
      <p>左侧选择一个配置，或点击 + 新建</p>
    </div>

    <template v-else>
      <div class="home-indicator" :class="statusClass">
        <Icon :name="icon" :size="30" />
      </div>

      <div class="home-status">{{ statusText }}</div>
      <div class="home-gateway mono" :title="gateway">{{ gateway }}</div>

      <div v-if="showLive && (state.conn.tunnelIp || elapsed)" class="home-meta mono">
        <span v-if="state.conn.tunnelIp">{{ state.conn.tunnelIp }}</span>
        <span v-if="state.conn.tunnelIp && elapsed" class="faint">·</span>
        <span v-if="elapsed">已连接 {{ elapsed }}</span>
      </div>

      <div class="home-actions">
        <button
          v-if="!active"
          class="btn btn-primary btn-lg home-btn"
          :disabled="!canConnect"
          @click="connectSelected()"
        >
          <Icon name="power" :size="16" /> 连接
        </button>
        <template v-else>
          <button
            class="btn btn-danger btn-lg home-btn"
            :disabled="st === 'disconnecting'"
            @click="disconnect()"
          >
            <Icon name="x" :size="16" /> 断开
          </button>
          <button
            v-if="st !== 'connected'"
            class="btn btn-ghost"
            title="立即 SIGKILL / taskkill /F，用于解决进程卡死"
            @click="forceKill()"
          >
            强制终止
          </button>
        </template>
      </div>

      <div v-if="handshakeSlow" class="home-note warn">
        握手已超过 {{ Math.round(HANDSHAKE_WARN_MS / 1000) }} 秒仍未建立隧道。
        <a @click="goDiag">查看日志</a>
      </div>

      <div v-if="errorSummary" class="home-note error">
        <div class="home-note-text">{{ errorSummary }}</div>
        <a @click="goDiag">查看完整日志与诊断 →</a>
      </div>

      <div v-if="envBlocking && !showLive" class="home-note warn">
        运行环境有 {{ state.env?.warnings.length }} 项问题，可能无法连接。
        <a @click="goDiag">查看详情</a>
      </div>
    </template>
  </section>
</template>

<style scoped>
.home {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 24px 32px;
  overflow-y: auto;
}

/* 顶部提示条：辅助进程授权状态（绝对定位，不干扰中央内容的居中） */
.home-banner {
  position: absolute;
  top: 10px;
  left: 10px;
  right: 10px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 11px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  line-height: 1.45;
  border: 1px solid var(--border);
  background: var(--panel-2);
  color: var(--text-dim);
}

.home-banner.warn {
  border-color: color-mix(in srgb, var(--warn) 45%, transparent);
  background: color-mix(in srgb, var(--warn) 12%, transparent);
  color: var(--warn);
}

.home-banner.error {
  border-color: color-mix(in srgb, var(--error) 45%, transparent);
  background: color-mix(in srgb, var(--error) 12%, transparent);
  color: var(--error);
}

.home-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--text-faint);
}

.home-indicator {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  border: 2px solid var(--border-strong);
  color: var(--text-faint);
  margin-bottom: 4px;
}
.home-indicator.handshaking,
.home-indicator.disconnecting {
  border-color: var(--warn);
  color: var(--warn);
  background: var(--warn-soft);
}
.home-indicator.connected {
  border-color: var(--success);
  color: var(--success);
  background: var(--success-soft);
}
.home-indicator.error {
  border-color: var(--error);
  color: var(--error);
  background: var(--error-soft);
}
.home-indicator .icon {
  width: 30px;
  height: 30px;
}

.home-status {
  font-size: 18px;
  font-weight: 600;
}

.home-gateway {
  font-size: 13px;
  color: var(--text-dim);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
}

.home-meta {
  display: flex;
  gap: 8px;
  font-size: 12px;
  color: var(--text-dim);
  user-select: text;
}

.home-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 10px;
}

.home-btn {
  min-width: 140px;
}

.home-note {
  margin-top: 8px;
  max-width: 420px;
  width: 100%;
  padding: 10px 12px;
  border-radius: var(--radius);
  font-size: 12.5px;
  line-height: 1.6;
  text-align: center;
}
.home-note.warn {
  background: var(--warn-soft);
  color: var(--text);
}
.home-note.error {
  background: var(--error-soft);
  color: var(--text);
}
.home-note-text {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--error);
  word-break: break-word;
  margin-bottom: 4px;
  user-select: text;
}
.home-note a {
  color: var(--accent);
  cursor: pointer;
  white-space: nowrap;
}
.home-note a:hover {
  text-decoration: underline;
}
</style>
