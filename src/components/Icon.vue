<script setup lang="ts">
/** 内联 SVG 图标集：零依赖，统一 stroke 风格。 */
withDefaults(defineProps<{ name: string; size?: number }>(), { size: 15 });

const PATHS: Record<string, string> = {
  power: `<path d="M12 3v9"/><path d="M18.4 6.6a9 9 0 1 1-12.8 0"/>`,
  shield: `<path d="M12 3l7 3v6c0 4.2-3 7.4-7 9-4-1.6-7-4.8-7-9V6z"/>`,
  x: `<path d="M6 6l12 12M18 6L6 18"/>`,
  plus: `<path d="M12 5v14M5 12h14"/>`,
  edit: `<path d="M4 20h4L20 8l-4-4L4 16z"/><path d="M14 6l4 4"/>`,
  trash: `<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>`,
  copy: `<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a1 1 0 0 1 1-1h9"/>`,
  search: `<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>`,
  upload: `<path d="M12 16V4M7 9l5-5 5 5M4 20h16"/>`,
  download: `<path d="M12 4v12M7 11l5 5 5-5M4 20h16"/>`,
  sun: `<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M19 5l-1.5 1.5M6.5 17.5L5 19"/>`,
  moon: `<path d="M20 14a8 8 0 1 1-10-10 7 7 0 0 0 10 10z"/>`,
  refresh: `<path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4v5h-5"/>`,
  terminal: `<path d="M5 6l5 5-5 5"/><path d="M13 17h6"/>`,
  alert: `<path d="M12 3l9 16H3z"/><path d="M12 9v5M12 17h.01"/>`,
  check: `<path d="M5 13l4 4L19 7"/>`,
  kill: `<path d="M12 3a7 7 0 0 0-7 7v3.5l2 2V19h10v-3.5l2-2V10a7 7 0 0 0-7-7z"/><path d="M9 11h.01M15 11h.01M10 20v1M14 20v1"/>`,
  // 设置齿轮：实心 8 齿 + 中心孔（常规齿轮轮廓）。原先是 8 角圆点星形，
  // 15px 下像颗糖果，辨识度差。
  settings: `<path fill="currentColor" stroke="none" d="M19.14 12.94a7.07 7.07 0 0 0 0-1.88l2.03-1.58a.5.5 0 0 0 .12-.64l-1.92-3.32a.5.5 0 0 0-.61-.22l-2.39.96a7.03 7.03 0 0 0-1.62-.94l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.5.42l-.36 2.54c-.59.24-1.13.56-1.62.94l-2.39-.96a.5.5 0 0 0-.61.22L2.74 8.84a.5.5 0 0 0 .12.64l2.03 1.58a7.07 7.07 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.64l1.92 3.32c.13.22.39.31.61.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.04.24.25.42.5.42h3.84c.25 0 .46-.18.5-.42l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.09.48 0 .61-.22l1.92-3.32a.5.5 0 0 0-.12-.64zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z"/>`,
  info: `<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 8h.01"/>`,
  link: `<path d="M10 13a5 5 0 0 0 7 0l2-2a5 5 0 0 0-7-7l-1 1"/><path d="M14 11a5 5 0 0 0-7 0l-2 2a5 5 0 0 0 7 7l1-1"/>`,
  monitor: `<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>`,
  folder: `<path d="M3 7h6l2 2h10v10H3z"/>`,
  chevron: `<path d="M9 6l6 6-6 6"/>`,
};
</script>

<template>
  <svg
    class="icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    v-html="PATHS[name] ?? ''"
  />
</template>