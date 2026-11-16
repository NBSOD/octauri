<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import TopBar from "./components/TopBar.vue";
import ProfileList from "./components/ProfileList.vue";
import ControlPanel from "./components/ControlPanel.vue";
import LogPanel from "./components/LogPanel.vue";
import ProfileDialog from "./components/ProfileDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import AboutDialog from "./components/AboutDialog.vue";
import AlertDialog from "./components/AlertDialog.vue";
import PasswordDialog from "./components/PasswordDialog.vue";
import Toasts from "./components/Toasts.vue";
import { api } from "./api";
import {
  alertError,
  connectWith,
  initApp,
  pendingPasswordPrompt,
  saveProfile,
  state,
} from "./store";
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

async function submitPassword(password: string, remember: boolean) {
  passwordOpen.value = false;
  const p = passwordProfile.value;
  if (!p) return;
  if (remember) {
    try {
      await api.credentialSet(p.id, password);
      state.credentialFlags[p.id] = true;
      // 用户在此勾选「记住」，需同步回配置，否则下次连接仍会追问
      if (!p.rememberPassword) {
        await saveProfile({ ...p, rememberPassword: true });
      }
    } catch (e) {
      alertError("保存密码失败", e, "系统钥匙串不可用时，可改用「每次手动输入」模式。");
    }
  }
  await connectWith(p.id, password);
}

onMounted(() => {
  void initApp();
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
    <AlertDialog />
    <Toasts />
  </div>
</template>