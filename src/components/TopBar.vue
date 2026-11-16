<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";
import { connStateLabel, refreshEnv, setTheme, state } from "../store";

const emit = defineEmits<{
  (e: "open-settings"): void;
  (e: "open-about"): void;
}>();

const statusText = computed(() => connStateLabel(state.conn.state));
const pid = computed(() => state.conn.pid);

const envOk = computed(() => {
  const env = state.env;
  if (!env) return false;
  return !!env.openconnectPath && env.elevated && env.warnings.length === 0;
});

const envTitle = computed(() => {
  const env = state.env;
  if (!env) return "环境未探测";
  if (!env.openconnectPath) return "未找到 openconnect";
  if (!env.elevated) return "缺少管理员权限";
  return env.openconnectVersion ?? env.openconnectPath;
});

function toggleTheme() {
  setTheme(state.prefs.theme === "dark" ? "light" : "dark");
}
</script>

<template>
  <header class="topbar">
    <div class="brand">
      <Icon name="shield" class="brand-mark" :size="18" />
      <span>Octauri</span>
    </div>

    <div class="pill" :class="state.conn.state">
      <span class="dot" />
      <span>{{ statusText }}</span>
      <span v-if="pid" class="mono faint">pid {{ pid }}</span>
    </div>

    <div class="spacer" />

    <button
      class="btn btn-ghost"
      :class="{ 'env-bad': state.env && !envOk }"
      :title="envTitle"
      @click="refreshEnv()"
    >
      <Icon name="refresh" :size="14" />
      <span class="mono env-text">{{ envTitle }}</span>
    </button>

    <button
      class="btn btn-icon btn-ghost"
      :title="state.prefs.theme === 'dark' ? '切换到浅色主题' : '切换到深色主题'"
      @click="toggleTheme"
    >
      <Icon :name="state.prefs.theme === 'dark' ? 'sun' : 'moon'" :size="15" />
    </button>

    <button class="btn btn-icon btn-ghost" title="设置" @click="emit('open-settings')">
      <Icon name="settings" :size="15" />
    </button>

    <button class="btn btn-icon btn-ghost" title="关于 / 开源许可" @click="emit('open-about')">
      <Icon name="info" :size="15" />
    </button>
  </header>
</template>

<style scoped>
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 10px;
  border-radius: 999px;
  border: 1px solid var(--border-strong);
  font-size: 12px;
  color: var(--text-dim);
}

.pill .dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--text-faint);
}

.pill.handshaking {
  border-color: var(--warn);
  color: var(--warn);
  background: var(--warn-soft);
}

.pill.handshaking .dot {
  background: var(--warn);
}

.pill.connected {
  border-color: var(--success);
  color: var(--success);
  background: var(--success-soft);
}

.pill.connected .dot {
  background: var(--success);
}

.pill.error {
  border-color: var(--error);
  color: var(--error);
  background: var(--error-soft);
}

.pill.error .dot {
  background: var(--error);
}

.pill.disconnecting {
  border-color: var(--warn);
  color: var(--warn);
  background: var(--warn-soft);
}

.pill.disconnecting .dot {
  background: var(--warn);
}

.env-text {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11.5px;
}

.env-bad {
  color: var(--warn);
}
</style>