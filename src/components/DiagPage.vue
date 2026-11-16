<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";
import LogPanel from "./LogPanel.vue";
import { refreshEnv, state } from "../store";

const env = computed(() => state.env);

const ocText = computed(() => {
  const e = env.value;
  if (!e?.openconnectPath) return "未找到";
  const m = e.openconnectVersion?.match(/v[\d.]+/);
  return m ? m[0] : "已找到";
});
</script>

<template>
  <section class="diag">
    <div class="diag-env">
      <div class="diag-env-head">
        <span class="diag-title">运行环境</span>
        <span class="spacer" />
        <button class="btn btn-ghost" style="height: 24px" @click="refreshEnv()">
          <Icon name="refresh" :size="13" /> 重新探测
        </button>
      </div>

      <div v-if="!env" class="hint">正在探测…</div>
      <template v-else>
        <div class="diag-metrics">
          <div class="metric" :title="env.openconnectPath ?? ''">
            <div class="metric-label">openconnect</div>
            <div class="metric-value" :class="env.openconnectPath ? 'ok' : 'bad'">{{ ocText }}</div>
          </div>
          <div class="metric">
            <div class="metric-label">权限</div>
            <div class="metric-value" :class="env.elevated ? 'ok' : 'bad'">
              {{ env.elevated ? "已提权" : "未提权" }}
            </div>
          </div>
          <div class="metric" :title="env.tunDetail">
            <div class="metric-label">虚拟网卡</div>
            <div class="metric-value" :class="env.tunReady ? 'ok' : 'bad'">
              {{ env.tunReady ? "可用" : "不可用" }}
            </div>
          </div>
          <div v-if="env.wintunReady !== null" class="metric" :title="env.wintunDetail">
            <div class="metric-label">wintun</div>
            <div class="metric-value" :class="env.wintunReady ? 'ok' : 'bad'">
              {{ env.wintunReady ? "就绪" : "缺失" }}
            </div>
          </div>
          <div class="metric">
            <div class="metric-label">平台</div>
            <div class="metric-value">{{ env.platform }}</div>
          </div>
        </div>

        <div v-if="env.openconnectPath" class="diag-path mono">{{ env.openconnectPath }}</div>

        <div v-if="env.warnings.length > 0" class="banner error" style="margin-top: 8px">
          <Icon name="alert" />
          <div>
            <div v-for="(w, i) in env.warnings" :key="i">· {{ w }}</div>
            <pre v-if="!env.elevated" style="margin-top: 6px; color: var(--text-dim)">{{ env.elevationHint }}</pre>
          </div>
        </div>
      </template>
    </div>

    <LogPanel class="diag-log" />
  </section>
</template>

<style scoped>
.diag {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.diag-env {
  flex: none;
  padding: 12px 14px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-elev);
}

.diag-env-head {
  display: flex;
  align-items: center;
  margin-bottom: 10px;
}

.diag-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-dim);
  letter-spacing: 0.4px;
}

.spacer {
  flex: 1;
}

.diag-metrics {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
  gap: 8px;
}

.metric {
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 7px 10px;
  min-width: 0;
}

.metric-label {
  font-size: 11px;
  color: var(--text-faint);
}

.metric-value {
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.metric-value.ok {
  color: var(--success);
}
.metric-value.bad {
  color: var(--error);
}

.diag-path {
  margin-top: 8px;
  font-size: 11px;
  color: var(--text-faint);
  word-break: break-all;
  user-select: text;
}

/* 复用日志面板：在诊断页铺满剩余空间 */
.diag-log {
  flex: 1;
  width: auto !important;
  max-width: none;
  min-width: 0;
  border-left: none;
}
</style>
