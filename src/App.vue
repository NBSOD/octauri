<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import TopBar from "./components/TopBar.vue";
import ProfileList from "./components/ProfileList.vue";
import ControlPanel from "./components/ControlPanel.vue";
import LogPanel from "./components/LogPanel.vue";
import ProfileDialog from "./components/ProfileDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import AboutDialog from "./components/AboutDialog.vue";
import AlertDialog from "./components/AlertDialog.vue";
import PasswordDialog from "./components/PasswordDialog.vue";
import CertDialog from "./components/CertDialog.vue";
import Toasts from "./components/Toasts.vue";
import { api } from "./api";
import { connectWith, initApp, pendingPasswordPrompt, state, toast } from "./store";
import type { Profile } from "./types";

const profileDialog = ref(false);
const editing = ref<Profile | null>(null);
const settingsOpen = ref(false);
const aboutOpen = ref(false);
const passwordOpen = ref(false);
const passwordProfile = ref<Profile | null>(null);

/** 日志面板宽度，支持拖拽调整并记忆 */
const LOG_MIN = 300;
const logWidth = ref(Number(localStorage.getItem("oc.logWidth")) || 440);
const dragging = ref(false);

const logStyle = computed(() => ({ width: `${logWidth.value}px` }));

function startDrag(e: MouseEvent) {
  dragging.value = true;
  const startX = e.clientX;
  const startW = logWidth.value;

  const onMove = (ev: MouseEvent) => {
    const next = startW - (ev.clientX - startX);
    const max = Math.round(window.innerWidth * 0.7);
    logWidth.value = Math.min(max, Math.max(LOG_MIN, next));
  };
  const onUp = () => {
    dragging.value = false;
    localStorage.setItem("oc.logWidth", String(logWidth.value));
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

function openNew() {
  editing.value = null;
  profileDialog.value = true;
}

function openEdit(p: Profile) {
  editing.value = p;
  profileDialog.value = true;
}

// 连接时若缺少凭据，由 store 触发密码框
watch(
  () => pendingPasswordPrompt.token,
  () => {
    const p = state.profiles.find((x) => x.id === pendingPasswordPrompt.id) ?? null;
    if (!p) return;
    passwordProfile.value = p;
    passwordOpen.value = true;
  },
);

async function submitPassword(password: string) {
  passwordOpen.value = false;
  const p = passwordProfile.value;
  if (!p) return;
  await connectWith(p.id, password);
}

/**
 * 接管复制：选中文本后按 Cmd+C / Ctrl+C（或用菜单「拷贝」）时，
 * 把选区交给后端写入登录用户的剪贴板。
 * 输入框内的复制保持默认行为不受影响（那里通常是用户自己的内容）。
 */
function onCopy(e: ClipboardEvent) {
  const target = e.target as HTMLElement | null;
  if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) {
    const el = target as HTMLInputElement | HTMLTextAreaElement;
    const sel = el.value.substring(el.selectionStart ?? 0, el.selectionEnd ?? 0);
    if (!sel) return;
    e.preventDefault();
    void api.clipboardWrite(sel).catch((err) => toast(`复制失败：${String(err)}`, "error"));
    return;
  }
  const text = window.getSelection()?.toString() ?? "";
  if (!text) return;
  e.preventDefault();
  void api
    .clipboardWrite(text)
    .then(() => toast("已复制", "success", 1200))
    .catch((err) => toast(`复制失败：${String(err)}`, "error"));
}

onMounted(() => {
  document.addEventListener("copy", onCopy);
  void initApp();
});

onBeforeUnmount(() => {
  document.removeEventListener("copy", onCopy);
});
</script>

<template>
  <div class="app" :class="{ dragging }">
    <TopBar
      @open-settings="settingsOpen = true"
      @open-about="aboutOpen = true"
    />

    <div class="main">
      <ProfileList @new="openNew" @edit="openEdit" />
      <ControlPanel />
      <div class="splitter" :class="{ dragging }" @mousedown.prevent="startDrag" />
      <LogPanel :style="logStyle" />
    </div>

    <ProfileDialog
      v-if="profileDialog"
      :profile="editing"
      @close="profileDialog = false"
    />
    <SettingsDialog v-if="settingsOpen" @close="settingsOpen = false" />
    <AboutDialog v-if="aboutOpen" @close="aboutOpen = false" />
    <PasswordDialog
      v-if="passwordOpen && passwordProfile"
      :profile="passwordProfile"
      @submit="submitPassword"
      @cancel="passwordOpen = false"
    />
    <CertDialog />
    <AlertDialog />
    <Toasts />
  </div>
</template>