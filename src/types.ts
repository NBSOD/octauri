/** 与 Rust 端共享的数据结构定义（字段名与 serde camelCase 对齐）。 */

export type AuthMethod = "password" | "certificate";

export interface Profile {
  id: string;
  name: string;
  gateway: string;
  username: string;
  authMethod: AuthMethod;
  protocol: string;
  certificatePath: string;
  keyPath: string;
  extraArgs: string;
  openconnectPath: string;
  /** 已固定的服务器证书指纹，空表示走正常 CA 校验 */
  servercert: string;
  createdAt: number;
  updatedAt: number;
}

export type ConnStateName =
  | "idle"
  | "handshaking"
  | "connected"
  | "disconnecting"
  | "error";

export interface StateSnapshot {
  state: ConnStateName;
  profileId: string | null;
  profileName: string | null;
  pid: number | null;
  since: number;
  message: string;
  /** 隧道内分配到的 IP，已连通后才有值 */
  tunnelIp: string | null;
  /** 本次连接的网关地址 */
  gateway: string | null;
}

export type LogLevel = "info" | "warn" | "error" | "success";

export interface LogLine {
  seq: number;
  ts: number;
  stream: "stdout" | "stderr" | "app" | string;
  level: LogLevel;
  text: string;
  tag: string | null;
}

export interface EnvReport {
  platform: string;
  openconnectPath: string | null;
  openconnectVersion: string | null;
  candidates: string[];
  elevated: boolean;
  elevationHint: string;
  tunReady: boolean;
  tunDetail: string;
  wintunReady: boolean | null;
  wintunDetail: string;
  warnings: string[];
}

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  arch: string;
  license: string;
  openconnectLicense: string;
  copyright: string;
}

export interface Prefs {
  theme: "light" | "dark";
  openconnectPath: string | null;
  logRenderLimit: number;
}

export const emptyProfile = (): Profile => ({
  id: "",
  name: "",
  gateway: "",
  username: "",
  authMethod: "password",
  protocol: "",
  certificatePath: "",
  keyPath: "",
  extraArgs: "",
  openconnectPath: "",
  servercert: "",
  createdAt: 0,
  updatedAt: 0,
});
/** 服务器证书校验失败时由后端推送 */
export interface CertPrompt {
  profileId: string;
  reason: string;
  pin: string;
}

/** root 辅助进程状态（macOS） */
export type HelperState = "starting" | "ready" | "error";
