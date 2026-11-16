<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import Icon from "./Icon.vue";
import { api } from "../api";
import { alertError, clearLogs, exportLogs, state, toast } from "../store";
import type { LogLine } from "../types";

type Filter = "all" | "error" | "warn";

const query = ref("");
const filter = ref<Filter>("all");
const autoScroll = ref(true);
const bodyRef = ref<HTMLElement | null>(null);
const atBottom = ref(true);

const filtered = computed<LogLine[]>(() => {
  const q = query.value.trim().toLowerCase();
  return state.logs.filter((l) => {
    if (filter.value === "error" && l.level !== "error") return false;
    if (filter.value === "warn" && l.level !== "warn" && l.level !== "error") return false;
    if (!q) return true;
    return l.text.toLowerCase().includes(q) || (l.tag ?? "").includes(q);
  });
});

const errorCount = computed(() => state.logs.filter((l) => l.level === "error").length);
const warnCount = computed(() => state.logs.filter((l) => l.level === "warn").length);

function fmtTime(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function escapeHtml(s: string): string {
  return s.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c,
  );
}

/** 先转义再插入 <mark>，避免日志内容被当作 HTML 执行 */
function renderText(text: string): string {
  const safe = escapeHtml(text);
  const q = query.value.trim();
  if (!q) return safe;
  const pattern = escapeHtml(q).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  try {
    return safe.replace(new RegExp(pattern, "gi"), (m) => `<mark>${m}</mark>`);
  } catch {
    return safe;
  }
}

function onScroll() {
  const el = bodyRef.value;
  if (!el) return;
  atBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
}

function scrollToBottom() {
  const el = bodyRef.value;
  if (!el) return;
  el.scrollTop = el.scrollHeight;
  atBottom.value = true;
}

// 新日志到达时，仅在用户位于底部（或开启自动滚动）时跟随
watch(
  () => state.logTick,
  async () => {
    if (!autoScroll.value || !atBottom.value) return;
    await nextTick();
    scrollToBottom();
  },
);

async function copyAll() {
  const list = filtered.value;
  if (list.length === 0) {
    toast("没有可复制的日志", "warn");
    return;
  }
  const text = list.map((l) => `[${fmtTime(l.ts)}] ${l.text}`).join("\n");
  let ok = false;
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      ok = true;
    }
  } catch {
    ok = false;
  }
  if (!ok) {
    // 回退方案：隐藏 textarea + execCommand
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    try {
      ok = document.execCommand("copy");
    } catch {
      ok = false;
    }
    document.body.removeChild(ta);
  }
  toast(ok ? `已复制 ${list.length} 行日志` : "复制失败，请手动选择文本", ok ? "success" : "error");
}

async function doExport() {
  try {
    const path = await save({
      title: "导出日志",
      defaultPath: `openconnect-${Date.now()}.log`,
      filters: [{ name: "日志文件", extensions: ["log", "txt"] }],
    });
    if (path) await exportLogs(path);
  } catch (e) {
    alertError("导出日志失败", e);
  }
}

async function refreshFromBackend() {
  try {
    state.logs = await api.logsTail(state.prefs.logRenderLimit);
    state.logTick++;
    toast("已重新载入后端日志缓冲", "info");
  } catch (e) {
    alertError("读取日志失败", e);
  }
}
</script>

<template>
  <aside class="col-right">
    <div class="log-head">
      <span class="log-title">日志</span>
      <div class="log-search">
        <Icon name="search" :size="13" />
        <input v-model="query" type="text" placeholder="检索关键字 / 标签…" spellcheck="false" />
      </div>
      <button class="btn btn-icon btn-ghost" title="导出日志到文件" @click="doExport">
        <Icon name="download" :size="14" />
      </button>
    </div>

    <div class="log-toolbar">
      <button class="chip" :class="{ active: filter === 'all' }" @click="filter = 'all'">
        全部 {{ state.logs.length }}
      </button>
      <button class="chip" :class="{ active: filter === 'error' }" @click="filter = 'error'">
        错误 {{ errorCount }}
      </button>
      <button class="chip" :class="{ active: filter === 'warn' }" @click="filter = 'warn'">
        警告 {{ warnCount }}
      </button>
      <span class="spacer" />
      <label class="check" style="font-size: 11px">
        <input v-model="autoScroll" type="checkbox" /> 自动滚动
      </label>
    </div>

    <div ref="bodyRef" class="log-body" @scroll="onScroll">
      <div v-if="filtered.length === 0" class="empty" style="padding: 24px 12px">
        {{ state.logs.length === 0 ? "暂无日志。连接后将实时显示 openconnect 输出。" : "没有匹配的日志行。" }}
      </div>
      <div
        v-for="l in filtered"
        :key="l.seq"
        class="log-line"
        :class="[l.level, l.stream === 'app' ? 'is-app' : '']"
      >
        <span class="log-time">{{ fmtTime(l.ts) }}</span>
        <span v-if="l.tag" class="log-tag" :class="l.tag">{{ l.tag }}</span>
        <span class="log-text" v-html="renderText(l.text)" />
      </div>
    </div>

    <div class="log-foot">
      <span class="mono">{{ filtered.length }} 行</span>
      <span class="spacer" />
      <button v-if="!atBottom" class="btn btn-ghost" style="height: 24px" @click="scrollToBottom">
        回到底部
      </button>
      <button class="btn btn-ghost" style="height: 24px" title="从后端缓冲重新载入" @click="refreshFromBackend">
        <Icon name="refresh" :size="13" />
      </button>
      <button class="btn btn-ghost" style="height: 24px" title="复制当前视图全部日志" @click="copyAll">
        <Icon name="copy" :size="13" /> 复制
      </button>
      <button class="btn btn-ghost" style="height: 24px" title="清空日志缓冲" @click="clearLogs()">
        <Icon name="trash" :size="13" /> 清空
      </button>
    </div>
  </aside>
</template>

<style scoped>
.log-toolbar .spacer {
  flex: 1;
}
</style>