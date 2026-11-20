/**
 * 全局状态与业务动作（不引入 Pinia，保持零额外依赖）。
 *
 * 日志采用「批量合并 + 定时刷新」写入响应式数组，避免高频 push 触发
 * 过多渲染；同时按 user limit 裁剪，配合后端环形缓冲形成双重上限。
 */
import { computed, reactive } from "vue";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorText } from "./api";
import type {
  AppInfo,
  CertPrompt,
  ConnStateName,
  EnvReport,
  GroupPrompt,
  LogLine,
  Prefs,
  Profile,
  StateSnapshot,
  ThemePref,
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
  conn: {
    state: "idle" as ConnStateName,
    profileId: null,
    profileName: null,
    pid: null,
    since: 0,
    message: "",
    tunnelIp: null,
    gateway: null,
  } as StateSnapshot,
  /** 当前页面：home = 连接，diag = 诊断（日志 + 环境） */
  page: "home" as "home" | "diag",
  logs: [] as LogLine[],
  env: null as EnvReport | null,
  info: null as AppInfo | null,
  prefs: {
    theme: "system",
    openconnectPath: null,
    logRenderLimit: 3000,
  } as Prefs,
  /** 实际生效的主题（由 prefs.theme 与系统偏好共同决定） */
  theme: "dark" as "light" | "dark",
  /** 单调递增，供日志面板判断是否需要自动滚动 */
  logTick: 0,
  busy: false,
  toasts: [] as Toast[],
  alert: null as AlertBox | null,
  /** 服务器证书待确认 */
  certPrompt: null as CertPrompt | null,
  /** 服务器要求选择 Group（Cisco ASA 多组） */
  groupPrompt: null as GroupPrompt | null,
  /**
   * root 辅助进程状态（macOS）。GUI 以普通用户运行，openconnect 由它以 root 拉起。
   * 授权在启动时进行一次，失败时连接不可用。
   */
  helper: {
    state: "starting" as "starting" | "ready" | "error",
    error: "",
  },
  /** 配置目录不可写时的信息（旧版以 root 运行的遗留） */
  dataDirIssue: null as { path: string; fix: string } | null,
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

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

/** 把 "system" 解析成实际的 light/dark。 */
function resolveTheme(pref: ThemePref): "light" | "dark" {
  if (pref === "system") return systemDark.matches ? "dark" : "light";
  return pref;
}

/**
 * 同时同步原生窗口主题（Windows 标题栏 / macOS 外观）。
 * 失败时忽略：老系统或缺少 window 权限不应影响界面本身。
 */
function syncWindowTheme(pref: ThemePref) {
  void getCurrentWindow()
    .setTheme(pref === "system" ? null : pref)
    .catch(() => undefined);
}

function applyTheme(pref: ThemePref) {
  const theme = resolveTheme(pref);
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  state.theme = theme;
  syncWindowTheme(pref);
}

// 系统主题变化时，只有「跟随系统」模式需要重新应用
systemDark.addEventListener("change", () => {
  if (state.prefs.theme === "system") applyTheme("system");
});

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
  await listen<CertPrompt>("oc://cert-prompt", (e) => {
    state.certPrompt = e.payload;
  });
  await listen<GroupPrompt>("oc://group-prompt", (e) => {
    state.groupPrompt = e.payload;
  });
  await listen("oc://helper-ready", () => {
    state.helper.state = "ready";
    state.helper.error = "";
    void refreshEnv();
  });
  await listen<string>("oc://helper-error", (e) => {
    state.helper.state = "error";
    state.helper.error = String(e.payload ?? "未知错误");
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

  // 恢复辅助进程状态：非 macOS 恒为 ready，避免界面一直显示「正在请求授权」
  try {
    const hs = await api.helperStatus();
    if (hs.error) {
      state.helper.state = "error";
      state.helper.error = hs.error;
    } else if (hs.state === "ready" || state.info?.platform !== "macos") {
      state.helper.state = "ready";
    }
  } catch {
    /* 保持默认 starting */
  }
  // 配置目录自检：不可写时后续所有保存都会失败，提前把原因和修法摆出来
  try {
    const ds = await api.dataDirStatus();
    if (!ds.writable) {
      state.dataDirIssue = {
        path: ds.path,
        fix: ds.foreignOwner
          ? `sudo chown -R "$(id -un)" "${ds.path}"`
          : `chmod u+w "${ds.path}"`,
      };
    }
  } catch {
    /* 自检失败不影响使用 */
  }

  state.ready = true;
}

// ------------------------------ Profile ------------------------------

export async function refreshProfiles() {
  try {
    state.profiles = await api.profilesList();
    if (!state.profiles.some((p) => p.id === state.selectedId)) {
      state.selectedId = state.profiles[0]?.id ?? "";
    }
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

/** 一个配置最多记住几个服务器证书指纹（与后端 MAX_SERVERCERT_PINS 保持一致） */
const MAX_SERVERCERT_PINS = 8;

/**
 * 解析配置里的指纹列表。分隔符支持 `,` / `;` / 换行，兼容旧版单指纹数据。
 *
 * 背景：同一套 ASA 集群的多个节点会各自签发自签名证书（实测两个节点证书
 * 完全不同），单指纹配置在 DNS 切换节点后会报
 * “None of the 1 fingerprint(s) specified via --servercert match”。
 */
export function parseServercertPins(raw: string): string[] {
  const out: string[] = [];
  for (const part of (raw ?? "").split(/[,;\r\n]/)) {
    const pin = part.trim();
    if (!pin.startsWith("pin-sha256:") || out.includes(pin)) continue;
    out.push(pin);
    if (out.length >= MAX_SERVERCERT_PINS) break;
  }
  return out;
}

/** 把新指纹并入列表：已存在不重复，超出上限时丢最旧的。 */
function mergeServercertPin(raw: string, pin: string): string {
  const pins = parseServercertPins(raw);
  const next = pin.trim();
  if (next.startsWith("pin-sha256:") && !pins.includes(next)) pins.push(next);
  return pins.slice(-MAX_SERVERCERT_PINS).join(",");
}

/** 用户确认信任该服务器证书：追加进信任列表后立即重连 */
export async function trustServerCert() {
  const prompt = state.certPrompt;
  if (!prompt) return;
  state.certPrompt = null;
  const p = state.profiles.find((x) => x.id === prompt.profileId);
  if (!p) return;
  const password = takePendingCredential(p.id);
  const saved = await saveProfile({
    ...p,
    servercert: mergeServercertPin(p.servercert, prompt.pin),
  });
  if (!saved) return;
  // 缓存已过期或不存在（证书模式除外）时，重新弹密码框，而不是发起必然失败的连接
  if (password === undefined && saved.authMethod === "password") {
    requestPassword(saved.id);
    return;
  }
  await connectWith(saved.id, password);
}

export function dismissCertPrompt() {
  state.certPrompt = null;
  pendingCredential = null;
}

/** 用户选择了 GROUP：写入配置并带 --authgroup 重连 */
export async function chooseGroup(group: string) {
  const prompt = state.groupPrompt;
  if (!prompt) return;
  state.groupPrompt = null;
  const p = state.profiles.find((x) => x.id === prompt.profileId);
  if (!p) return;
  const password = takePendingCredential(p.id);
  const saved = await saveProfile({ ...p, authGroup: group });
  if (!saved) return;
  if (password === undefined && saved.authMethod === "password") {
    requestPassword(saved.id);
    return;
  }
  await connectWith(saved.id, password, group);
}

export function dismissGroupPrompt() {
  state.groupPrompt = null;
  pendingCredential = null;
}

// ------------------------------ 连接控制 ------------------------------

/**
 * 最近一次连接使用的密码，仅存于内存，用于「信任证书后立即重连」。
 * 不落盘、不进响应式状态；重连取用后立即清空，60 秒后自动作废。
 */
let pendingCredential: { profileId: string; password: string; expires: number } | null = null;
const CREDENTIAL_TTL_MS = 60_000;

function takePendingCredential(profileId: string): string | undefined {
  const c = pendingCredential;
  pendingCredential = null;
  if (!c || c.profileId !== profileId || Date.now() > c.expires) return undefined;
  return c.password;
}

export async function connectWith(profileId: string, password?: string, authGroup?: string) {
  if (state.busy) return;
  state.busy = true;
  pendingCredential = password
    ? { profileId, password, expires: Date.now() + CREDENTIAL_TTL_MS }
    : null;
  try {
    const pid = await api.connect(profileId, password, authGroup);
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
  // 密码不落盘：除证书模式（口令可为空）外，一律弹窗输入
  if (p.authMethod === "certificate") {
    await connectWith(p.id);
    return;
  }
  requestPassword(p.id);
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

export function setTheme(theme: ThemePref) {
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