import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";

// 在 Vue 挂载前先按系统偏好定一次主题，避免浅色系统启动时闪一下深色
document.documentElement.dataset.theme = window.matchMedia("(prefers-color-scheme: dark)")
  .matches
  ? "dark"
  : "light";
document.documentElement.style.colorScheme = document.documentElement.dataset.theme;

createApp(App).mount("#app");