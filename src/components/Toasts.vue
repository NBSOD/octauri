<script setup lang="ts">
import { state } from "../store";
</script>

<template>
  <div class="toasts">
    <div v-for="t in state.toasts" :key="t.id" class="toast" :class="t.type">
      {{ t.text }}
    </div>
  </div>
</template>