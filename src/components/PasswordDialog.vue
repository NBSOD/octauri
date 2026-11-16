<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import type { Profile } from "../types";

defineProps<{ profile: Profile }>();
const emit = defineEmits<{
  (e: "submit", password: string): void;
  (e: "cancel"): void;
}>();

const password = ref("");
const inputRef = ref<HTMLInputElement | null>(null);
const show = ref(false);

onMounted(async () => {
  await nextTick();
  inputRef.value?.focus();
});

function submit() {
  if (!password.value) return;
  emit("submit", password.value);
}
</script>

<template>
  <div class="backdrop" @click.self="emit('cancel')">
    <div class="modal" style="max-width: 440px">
      <div class="modal-head">
        <Icon name="shield" :size="16" />
        <span>输入登录密码</span>
      </div>

      <div class="modal-body">
        <dl class="kv">
          <dt>配置</dt>
          <dd>{{ profile.name }}</dd>
          <dt>网关</dt>
          <dd>{{ profile.gateway }}</dd>
          <dt>用户名</dt>
          <dd>{{ profile.username || "—" }}</dd>
        </dl>

        <div class="field">
          <span class="label">密码</span>
          <div class="field-inline">
            <input
              ref="inputRef"
              v-model="password"
              :type="show ? 'text' : 'password'"
              placeholder="请输入密码"
              autocomplete="current-password"
              @keyup.enter="submit"
            />
            <button class="btn" @click="show = !show">
              {{ show ? "隐藏" : "显示" }}
            </button>
          </div>
        </div>

        <span class="hint">
          密码仅用于本次连接，不会保存到本地。它通过 stdin 直接传给 openconnect，
          不出现在进程参数列表中，也不会写入配置文件。
        </span>
      </div>

      <div class="modal-foot">
        <button class="btn" @click="emit('cancel')">取消</button>
        <button class="btn btn-primary" :disabled="!password" @click="submit">连接</button>
      </div>
    </div>
  </div>
</template>