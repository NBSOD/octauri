<script setup lang="ts">
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import Icon from "./Icon.vue";
import { state, toast, updatePrefs } from "../store";

const emit = defineEmits<{ (e: "close"): void }>();

const path = ref(state.prefs.openconnectPath ?? "");
const limit = ref(state.prefs.logRenderLimit);

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