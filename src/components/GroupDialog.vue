<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";
import { chooseGroup, dismissGroupPrompt, state } from "../store";

const prompt = computed(() => state.groupPrompt);
const profile = computed(
  () => state.profiles.find((p) => p.id === state.groupPrompt?.profileId) ?? null,
);
</script>

<template>
  <div v-if="prompt" class="backdrop" @click.self="dismissGroupPrompt">
    <div class="modal" style="max-width: 460px">
      <div class="modal-head">
        <Icon name="shield" :size="16" style="color: var(--warn)" />
        <span>选择 VPN Group</span>
      </div>

      <div class="modal-body">
        <dl class="kv">
          <dt>配置</dt>
          <dd>{{ profile?.name ?? "—" }}</dd>
          <dt>网关</dt>
          <dd>{{ profile?.gateway ?? "—" }}</dd>
        </dl>

        <p class="hint">
          服务器要求先选择 Group（Cisco ASA 多组场景）。选择后会写入当前配置，
          并以 <code>--authgroup</code> 自动重连，下次连接不再询问。
        </p>

        <div class="group-options">
          <button
            v-for="g in prompt.groups"
            :key="g"
            class="btn"
            @click="chooseGroup(g)"
          >
            {{ g }}
          </button>
        </div>
      </div>

      <div class="modal-foot">
        <button class="btn" @click="dismissGroupPrompt">取消</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.group-options {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.group-options .btn {
  height: 36px;
  justify-content: flex-start;
  padding: 0 14px;
  font-family: var(--font-mono);
}

code {
  font-family: var(--font-mono);
  font-size: 11.5px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 0 4px;
}
</style>