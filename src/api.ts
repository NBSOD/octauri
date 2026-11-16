/** 后端命令薄封装：统一错误信息格式化，避免各组件重复 try/catch。 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  EnvReport,
  LogLine,
  Prefs,
  Profile,
  StateSnapshot,
} from "./types";

/** 把后端返回的错误统一转为可读字符串。 */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),

  profilesList: () => invoke<Profile[]>("profiles_list"),
  profileSave: (profile: Profile) => invoke<Profile>("profile_save", { profile }),
  profileDelete: (id: string) => invoke<void>("profile_delete", { id }),
  profileImport: (path: string) => invoke<number>("profile_import", { path }),
  profileExport: (path: string, ids: string[]) =>
    invoke<void>("profile_export", { path, ids }),

  credentialStatus: (profileId: string) =>
    invoke<boolean>("credential_status", { profileId }),
  credentialSet: (profileId: string, password: string) =>
    invoke<void>("credential_set", { profileId, password }),
  credentialClear: (profileId: string) =>
    invoke<void>("credential_clear", { profileId }),

  envProbe: (profileId?: string) =>
    invoke<EnvReport>("env_probe", { profileId: profileId ?? null }),

  connect: (profileId: string, password?: string) =>
    invoke<number>("connect", { profileId, password: password ?? null }),
  disconnect: () => invoke<void>("disconnect"),
  forceKill: () => invoke<void>("force_kill"),

  connState: () => invoke<StateSnapshot>("conn_state"),
  logsTail: (limit?: number) => invoke<LogLine[]>("logs_tail", { limit: limit ?? null }),
  logsClear: () => invoke<void>("logs_clear"),
  logsExport: (path: string) => invoke<void>("logs_export", { path }),

  prefsGet: () => invoke<Prefs>("prefs_get"),
  prefsSet: (prefs: Prefs) => invoke<Prefs>("prefs_set", { prefs }),
};