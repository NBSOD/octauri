/**
 * 全局状态与业务动作（不引入 Pinia，保持零额外依赖）。
 *
 * 日志采用「批量合并 + 定时刷新」写入响应式数组，避免高频 push 触发
 * 过多渲染；同时按 user limit 裁剪，配合后端环形缓冲形成双重上限。
 */
import { computed, reactive } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
import type {
  AppInfo,
  ConnStateName,
  EnvReport,
  LogLine,
  Prefs,
  Profile,
  StateSnapshot,
} from "./types";

export interface Toast {
  id: number;
  type: "info" | "success" | "warn" | "error";
  text: string;
}

export interface AlertBox {
  title: string;
  detail: string;
  hint?: string;
}

export const state = reactive({
  ready: false,
  profiles: [] as Profile[],
  selectedId: "" as string,
  /** 各配置是否已在钥匙串中存有密码 */
  credentialFlags: {} as Record<string, boolean>,
  conn: {
    state: "idle" as ConnStateName,
    profileId: null,
    profileName: null,
    pid: null,
    since: 0,
    message: "",
  } as StateSnapshot,
  logs: [] as LogLine[],
  env: null as EnvReport | null,
  info: null as AppInfo | null,
  prefs: {
    theme: "dark",
    openconnectPath: null,
    logRenderLimit: 3000,
  } as Prefs,
  /** 单调递增，供日志面板判断是否需要自动滚动 */
  logTick: 0,
  busy: false,
  toasts: [] as Toast[],
  alert: null as AlertBox | null,
});

export const selectedProfile = computed<Profile | null>(
  () => state.profiles.find((p) => p.id === state.selectedId) ?? null,
);

export const isBusyState = computed(() =>
  ["handshaking", "connected", "disconnecting"].includes(state.conn.state),
);

/** 握手阶段超过该时长仍未连通，提示用户可能存在超时 */
export const HANDSHAKE_WARN_MS = 60_000;

// ------------------------------ 日志 ------------------------------

let pendingLogs: LogLine[] = [];
let flushScheduled = false;

function scheduleFlush() {
  if (flushScheduled) return;
  flushScheduled = true;
  requestAnimationFrame(() => {
    flushScheduled = false;
    if (pendingLogs.length === 0) return;
    const batch = pendingLogs;
    pendingLogs = [];
    // 分批 push，避免超大参数列表
    for (let i = 0; i < batch.length; i += 200) {
      state.logs.push(...batch.slice(i, i + 200));
    }
    const overflow = state.logs.length - state.prefs.logRenderLimit;
    if (overflow > 0) state.logs.splice(0, overflow);
    state.logTick++;
  });
}

function pushLog(line: LogLine) {
  pendingLogs.push(line);
  scheduleFlush();
}

// ------------------------------ 提示 ------------------------------

let toastSeq = 0;

export function toast(text: string, type: Toast["type"] = "info", ttl = 3200) {
  const item: Toast = { id: ++toastSeq, type, text };
  state.toasts.push(item);
  if (state.toasts.length > 4) state.toasts.shift();
  window.setTimeout(() => {
    const i = state.toasts.findIndex((t) => t.id === item.id);
    if (i >= 0) state.toasts.splice(i, 1);
  }, ttl);
}

export function alertError(title: string, err: unknown, hint?: string) {
  state.alert = { title, detail: errorText(err), hint };
}

export function closeAlert() {
  state.alert = null;
}

// ------------------------------ 主题 ------------------------------

function applyTheme(theme: "light" | "dark") {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

// ------------------------------ 初始化 ------------------------------

export async function initApp() {
  try {
    state.info = await api.appInfo();
  } catch {
    /* 关于页降级 */
  }

  try {
    state.prefs = await api.prefsGet();
  } catch (e) {
    console.warn("读取设置失败", e);
  }
  applyTheme(state.prefs.theme);

  // 事件订阅先于首次拉取，避免漏掉启动瞬间的日志
  await listen<LogLine>("oc://log", (e) => pushLog(e.payload));
  await listen<StateSnapshot>("oc://state", (e) => {
    state.conn = e.payload;
  });
  await listen("oc://tray-connect", () => {
    void connectSelected();
  });

  try {
    state.conn = await api.connState();
    const history = await api.logsTail(state.prefs.logRenderLimit);
    state.logs = history;
    state.logTick++;
  } catch (e) {
    console.warn("恢复运行状态失败", e);
  }

  await refreshProfiles();
  await refreshEnv();
  state.ready = true;
}

// ------------------------------ Profile ------------------------------

export async function refreshProfiles() {
  try {
    state.profiles = await api.profilesList();
    if (!state.profiles.some((p) => p.id === state.selectedId)) {
      state.selectedId = state.profiles[0]?.id ?? "";
    }
    // 批量查询凭据状态（数量通常个位数，开销可忽略）
    const flags: Record<string, boolean> = {};
    for (const p of state.profiles) {
      if (p.rememberPassword) {
        try {
          flags[p.id] = await api.credentialStatus(p.id);
        } catch {
          flags[p.id] = false;
        }
      }
    }
    state.credentialFlags = flags;
  } catch (e) {
    alertError("读取配置失败", e);
  }
}

export async function saveProfile(profile: Profile): Promise<Profile | null> {
  try {
    const saved = await api.profileSave(profile);
    await refreshProfiles();
    state.selectedId = saved.id;
    toast(`配置「${saved.name}」已保存`, "success");
    return saved;
  } catch (e) {
    alertError("保存配置失败", e);
    return null;
  }
}

export async function deleteProfile(id: string) {
  try {
    await api.profileDelete(id);
    await refreshProfiles();
    toast("配置已删除", "success");
  } catch (e) {
    alertError("删除配置失败", e);
  }
}

export async function importProfiles(path: string) {
  try {
    const n = await api.profileImport(path);
    await refreshProfiles();
    toast(`成功导入 ${n} 个配置`, "success");
  } catch (e) {
    alertError("导入失败", e);
  }
}

export async function exportProfiles(path: string, ids: string[]) {
  try {
    await api.profileExport(path, ids);
    toast("配置已导出", "success");
  } catch (e) {
    alertError("导出失败", e);
  }
}

/** 是否需要用户手动输入密码 */
export function needsPassword(profile: Profile): boolean {
  if (profile.authMethod === "certificate") return false;
  if (!profile.rememberPassword) return true;
  return !state.credentialFlags[profile.id];
}

// ------------------------------ 连接控制 ------------------------------

export async function connectWith(profileId: string, password?: string) {
  if (state.busy) return;
  state.busy = true;
  try {
    const pid = await api.connect(profileId, password);
    toast(`openconnect 已启动（pid ${pid}）`, "info");
  } catch (e) {
    alertError(
      "连接失败",
      e,
      "请检查网关地址、认证信息与 openconnect 路径；详细原因见日志面板。",
    );
  } finally {
    state.busy = false;
  }
}

/** 组件监听 token 变化，弹出密码输入框（用 token 支持重复触发） */
export const pendingPasswordPrompt = reactive({ id: "", token: 0 });

export function requestPassword(profileId: string) {
  pendingPasswordPrompt.id = profileId;
  pendingPasswordPrompt.token++;
}

export async function connectSelected() {
  const p = selectedProfile.value;
  if (!p) {
    toast("请先选择一个配置", "warn");
    return;
  }
  if (needsPassword(p)) {
    requestPassword(p.id);
    return;
  }
  await connectWith(p.id);
}

export async function disconnect() {
  try {
    await api.disconnect();
  } catch (e) {
    alertError("断开失败", e);
  }
}

export async function forceKill() {
  try {
    await api.forceKill();
    toast("已强制终止 openconnect 进程", "warn");
  } catch (e) {
    alertError("强制终止失败", e);
  }
}

// ------------------------------ 日志操作 ------------------------------

export async function clearLogs() {
  try {
    await api.logsClear();
    state.logs = [];
    toast("日志已清空", "info");
  } catch (e) {
    alertError("清空日志失败", e);
  }
}

export async function exportLogs(path: string) {
  try {
    await api.logsExport(path);
    toast("日志已导出", "success");
  } catch (e) {
    alertError("导出日志失败", e);
  }
}

// ------------------------------ 环境 / 设置 ------------------------------

export async function refreshEnv() {
  try {
    state.env = await api.envProbe(state.selectedId || undefined);
  } catch (e) {
    console.warn("环境探测失败", e);
  }
}

export async function updatePrefs(patch: Partial<Prefs>) {
  const next: Prefs = { ...state.prefs, ...patch };
  try {
    state.prefs = await api.prefsSet(next);
    applyTheme(state.prefs.theme);
    // 裁剪已有日志以匹配新的上限
    const overflow = state.logs.length - state.prefs.logRenderLimit;
    if (overflow > 0) state.logs.splice(0, overflow);
    await refreshEnv();
  } catch (e) {
    alertError("保存设置失败", e);
  }
}

export function setTheme(theme: "light" | "dark") {
  void updatePrefs({ theme });
}

export function connStateLabel(s: ConnStateName): string {
  switch (s) {
    case "idle":
      return "未连接";
    case "handshaking":
      return "握手进行中";
    case "connected":
      return "已连通";
    case "disconnecting":
      return "正在断开";
    case "error":
      return "异常断开";
  }
}