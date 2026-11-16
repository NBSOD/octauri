<script setup lang="ts">
import { ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import Icon from "./Icon.vue";
import {
  deleteProfile,
  exportProfiles,
  importProfiles,
  refreshEnv,
  state,
  toast,
} from "../store";
import type { Profile } from "../types";

const emit = defineEmits<{
  (e: "new"): void;
  (e: "edit", p: Profile): void;
}>();

const deletingId = ref("");

/** 仅在可执行文件路径变化时才重新探测，避免频繁切换配置触发子进程调用 */
let lastProbePath: string | null = null;

function select(p: Profile) {
  state.selectedId = p.id;
  if (lastProbePath !== p.openconnectPath) {
    lastProbePath = p.openconnectPath;
    void refreshEnv();
  }
}

function dotClass(p: Profile): string {
  if (state.conn.profileId !== p.id) return "";
  switch (state.conn.state) {
    case "connected":
      return "on";
    case "handshaking":
    case "disconnecting":
      return "busy";
    case "error":
      return "err";
    default:
      return "";
  }
}

async function doImport() {
  try {
    const path = await open({
      multiple: false,
      directory: false,
      title: "选择要导入的配置文件",
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (typeof path === "string") await importProfiles(path);
  } catch (e) {
    toast(`打开文件选择器失败：${String(e)}`, "error");
  }
}

async function doExport() {
  try {
    const path = await save({
      title: "导出配置",
      defaultPath: "openconnect-profiles.json",
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (!path) return;
    // 仅导出当前选中项；若未选中则导出全部
    const ids = state.selectedId ? [state.selectedId] : [];
    await exportProfiles(path, ids);
  } catch (e) {
    toast(`导出失败：${String(e)}`, "error");
  }
}

async function confirmDelete(id: string) {
  deletingId.value = "";
  await deleteProfile(id);
}
</script>

<template>
  <aside class="col-left">
    <div class="left-head">
      <span class="left-title">配置列表</span>
      <button class="btn btn-icon btn-ghost" title="新建配置" @click="emit('new')">
        <Icon name="plus" :size="15" />
      </button>
    </div>

    <div class="profile-list">
      <div v-if="state.profiles.length === 0" class="empty">
        还没有配置。<br />
        点击右上角 + 新建，或从文件导入。
      </div>

      <div
        v-for="p in state.profiles"
        :key="p.id"
        class="profile-item"
        :class="{ active: p.id === state.selectedId }"
        @click="select(p)"
      >
        <span class="profile-dot" :class="dotClass(p)" />
        <div class="profile-meta">
          <div class="profile-name">{{ p.name }}</div>
          <div class="profile-sub">{{ p.gateway }}</div>
        </div>

        <template v-if="deletingId === p.id">
          <button class="btn btn-danger" style="height: 24px; padding: 0 8px" @click.stop="confirmDelete(p.id)">
            删除
          </button>
          <button
            class="btn btn-ghost"
            style="height: 24px; padding: 0 6px"
            @click.stop="deletingId = ''"
          >
            取消
          </button>
        </template>
        <template v-else>
          <button
            class="btn btn-icon btn-ghost"
            style="height: 24px; width: 24px"
            title="编辑"
            @click.stop="emit('edit', p)"
          >
            <Icon name="edit" :size="13" />
          </button>
          <button
            class="btn btn-icon btn-ghost"
            style="height: 24px; width: 24px"
            title="删除"
            @click.stop="deletingId = p.id"
          >
            <Icon name="trash" :size="13" />
          </button>
        </template>
      </div>
    </div>

    <div class="left-foot">
      <button class="btn" title="从 JSON 文件导入配置" @click="doImport">
        <Icon name="upload" :size="14" /> 导入
      </button>
      <button
        class="btn"
        :disabled="state.profiles.length === 0"
        :title="state.selectedId ? '导出当前选中配置' : '导出全部配置'"
        @click="doExport"
      >
        <Icon name="download" :size="14" /> 导出
      </button>
    </div>
  </aside>
</template>