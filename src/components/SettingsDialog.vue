<script setup lang="ts">
import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import { state, toast, updatePrefs } from "../store";
import { fetchLatestRelease, isNewer, type LatestRelease } from "../update";

const emit = defineEmits<{ (e: "close"): void }>();

const path = ref(state.prefs.openconnectPath ?? "");
const limit = ref(state.prefs.logRenderLimit);

const currentVersion = computed(() => state.info?.version ?? "—");
const checking = ref(false);
const latest = ref<LatestRelease | null>(null);
const updateMessage = ref("");
const updateError = ref("");

async function checkUpdates() {
  if (checking.value) return;
  checking.value = true;
  latest.value = null;
  updateMessage.value = "";
  updateError.value = "";
  try {
    const release = await fetchLatestRelease(state.info?.platform);
    if (!release) {
      updateMessage.value = "仓库还没有已发布的 Release（草稿不计入）。";
      return;
    }
    if (isNewer(release.version, currentVersion.value)) {
      latest.value = release;
      updateMessage.value = `发现新版本 v${release.version}`;
    } else {
      updateMessage.value = `已是最新版本（v${currentVersion.value}）`;
    }
  } catch (e) {
    updateError.value = `检查更新失败：${String(e)}`;
  } finally {
    checking.value = false;
  }
}

async function openReleases() {
  try {
    await openUrl("https://github.com/NBSOD/octauri/releases");
  } catch (e) {
    toast(`打开发布页失败：${String(e)}`, "error");
  }
}

async function downloadUpdate() {
  if (!latest.value) return;
  try {
    await openUrl(latest.value.downloadUrl);
  } catch (e) {
    toast(`打开下载链接失败：${String(e)}`, "error");
  }
}

async function browse() {
  const p = await open({ multiple: false, title: "选择 openconnect 可执行文件" });
  if (typeof p === "string") path.value = p;
}

async function submit() {
  await updatePrefs({
    openconnectPath: path.value.trim() ? path.value.trim() : null,
    logRenderLimit: Math.min(20000, Math.max(200, Number(limit.value) || 3000)),
  });
  toast("设置已保存", "success");
  emit("close");
}
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="modal">
      <div class="modal-head">
        <Icon name="settings" :size="16" />
        <span>设置</span>
      </div>

      <div class="modal-body">
        <div class="field">
          <span class="label">外观主题</span>
          <div class="actions">
            <button
              class="btn"
              :class="{ 'btn-primary': state.prefs.theme === 'system' }"
              @click="updatePrefs({ theme: 'system' })"
            >
              <Icon name="monitor" :size="14" /> 跟随系统
            </button>
            <button
              class="btn"
              :class="{ 'btn-primary': state.prefs.theme === 'light' }"
              @click="updatePrefs({ theme: 'light' })"
            >
              <Icon name="sun" :size="14" /> 浅色
            </button>
            <button
              class="btn"
              :class="{ 'btn-primary': state.prefs.theme === 'dark' }"
              @click="updatePrefs({ theme: 'dark' })"
            >
              <Icon name="moon" :size="14" /> 深色
            </button>
          </div>
          <span class="hint">
            跟随系统时，界面与原生标题栏会随 Windows / macOS 的深浅色设置自动切换。
          </span>
        </div>

        <div class="field">
          <span class="label">版本与更新</span>
          <div class="actions">
            <span class="mono dim" style="font-size: 12px">当前 v{{ currentVersion }}</span>
            <button class="btn" :disabled="checking" @click="checkUpdates">
              <Icon name="refresh" :size="13" />
              {{ checking ? "检查中…" : "检查更新" }}
            </button>
            <button class="btn btn-ghost" @click="openReleases">打开发布页</button>
          </div>
          <span v-if="updateMessage" class="hint">{{ updateMessage }}</span>
          <span v-if="updateError" class="hint" style="color: var(--error)">{{ updateError }}</span>
          <button v-if="latest" class="btn btn-primary" @click="downloadUpdate">
            <Icon name="download" :size="13" /> 下载 v{{ latest.version }} 安装包
          </button>
          <span v-if="latest?.fileName" class="hint mono">{{ latest.fileName }}</span>
        </div>

        <div class="field">
          <span class="label">全局 openconnect 路径</span>
          <div class="field-inline">
            <input v-model="path" type="text" placeholder="留空则自动探测（PATH / Homebrew / 内置 resources）" spellcheck="false" />
            <button class="btn" @click="browse">
              <Icon name="folder" :size="13" /> 浏览
            </button>
          </div>
        </div>
        <span class="hint">
          优先级：单个配置内指定的路径 &gt; 此处全局路径 &gt; 自动探测。修改后会立即重新探测环境。
        </span>

        <label class="field">
          <span class="label">日志面板最多保留行数</span>
          <input v-model.number="limit" type="text" inputmode="numeric" />
          <span class="hint">
            仅影响界面渲染（200–20000）。后端另有固定 2000 条的滚动缓冲，长时间运行不会持续增长。
          </span>
        </label>

        <div v-if="state.env" class="field">
          <span class="label">当前生效的 openconnect</span>
          <span class="mono dim" style="font-size: 12px; word-break: break-all">
            {{ state.env.openconnectPath ?? "未找到" }}
          </span>
          <span v-if="state.env.openconnectVersion" class="hint">{{ state.env.openconnectVersion }}</span>
        </div>
      </div>

      <div class="modal-foot">
        <button class="btn" @click="emit('close')">关闭</button>
        <button class="btn btn-primary" @click="submit">保存</button>
      </div>
    </div>
  </div>
</template>