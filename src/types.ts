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
  createdAt: 0,
  updatedAt: 0,
});