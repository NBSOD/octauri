<script setup lang="ts">
import Icon from "./Icon.vue";
import { closeAlert, state } from "../store";
</script>

<template>
  <div v-if="state.alert" class="backdrop" @click.self="closeAlert">
    <div class="modal" style="max-width: 620px">
      <div class="modal-head">
        <Icon name="alert" :size="16" style="color: var(--error)" />
        <span>{{ state.alert.title }}</span>
      </div>
      <div class="modal-body">
        <div class="pre-wrap" style="color: var(--text)">{{ state.alert.detail }}</div>
        <div v-if="state.alert.hint" class="hint">{{ state.alert.hint }}</div>
      </div>
      <div class="modal-foot">
        <button class="btn btn-primary" @click="closeAlert">知道了</button>
      </div>
    </div>
  </div>
</template>