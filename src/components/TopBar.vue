<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";
import { setTheme, state } from "../store";

const emit = defineEmits<{
  (e: "open-settings"): void;
  (e: "open-about"): void;
}>();

/** 诊断页角标：环境有阻断问题，或连接异常断开 */
const diagAlert = computed(
  () => (state.env?.warnings.length ?? 0) > 0 || state.conn.state === "error",
);

const isDark = computed(() => state.theme === "dark");

function toggleTheme() {
  setTheme(isDark.value ? "light" : "dark");
}
</script>

<template>
  <header class="topbar">
    <div class="brand">
      <Icon name="shield" class="brand-mark" :size="18" />
      <span>Octauri</span>
    </div>

    <nav class="tabs">
      <button class="tab" :class="{ active: state.page === 'home' }" @click="state.page = 'home'">
        连接
        <span v-if="state.page !== 'home'" class="tab-dot" :class="state.conn.state" />
      </button>
      <button class="tab" :class="{ active: state.page === 'diag' }" @click="state.page = 'diag'">
        诊断
        <span v-if="diagAlert" class="tab-badge">!</span>
      </button>
    </nav>

    <div class="spacer" />

    <button
      class="btn btn-icon btn-ghost"
      :title="isDark ? '切换到浅色主题' : '切换到深色主题'"
      @click="toggleTheme"
    >
      <Icon :name="isDark ? 'sun' : 'moon'" :size="15" />
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
.tabs {
  display: flex;
  gap: 2px;
  margin-left: 18px;
  padding: 2px;
  border-radius: var(--radius);
  background: var(--panel-2);
  border: 1px solid var(--border);
}

.tab {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 14px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  font-size: 12.5px;
}
.tab:hover {
  color: var(--text);
}
.tab.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 500;
}

/* 在诊断页时，「连接」页签上的小圆点提示当前连接状态 */
.tab-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-faint);
}
.tab-dot.connected {
  background: var(--success);
}
.tab-dot.handshaking,
.tab-dot.disconnecting {
  background: var(--warn);
}
.tab-dot.error {
  background: var(--error);
}

.tab-badge {
  min-width: 14px;
  height: 14px;
  padding: 0 4px;
  border-radius: 7px;
  background: var(--error);
  color: #fff;
  font-size: 10px;
  line-height: 14px;
  font-weight: 600;
}
</style>