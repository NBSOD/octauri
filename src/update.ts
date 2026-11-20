/**
 * 轻量更新检查：只查询 GitHub 上「已发布」的最新 Release，
 * 发现新版本后由用户点击跳转到对应安装包下载页。
 *
 * 不依赖 Tauri updater、不需要签名密钥，也不会自动下载/安装。
 */
const REPO = "NBSOD/octauri";
const API = `https://api.github.com/repos/${REPO}`;

export interface LatestRelease {
  version: string;
  pageUrl: string;
  downloadUrl: string;
  fileName: string | null;
}

interface GithubAsset {
  name?: string;
  browser_download_url?: string;
}

interface GithubRelease {
  tag_name?: string;
  html_url?: string;
  draft?: boolean;
  prerelease?: boolean;
  assets?: GithubAsset[];
}

function parseVersion(v: string): number[] {
  return v
    .replace(/^v/i, "")
    .split(/[.+-]/)
    .map((x) => parseInt(x, 10) || 0);
}

/** 语义化比较：candidate 是否比 current 新 */
export function isNewer(candidate: string, current: string): boolean {
  const a = parseVersion(candidate);
  const b = parseVersion(current);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i] ?? 0;
    const y = b[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

function signalAfter(ms: number): AbortSignal {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), ms);
  return controller.signal;
}

/**
 * 查询最新发布版本。
 * 返回 null 表示仓库还没有已发布的 Release（草稿不算）。
 *
 * 先请求 /releases/latest；若它返回 404（某些网络/代理会缓存 404），
 * 再退回 /releases?per_page=1 列表接口。
 */
export async function fetchLatestRelease(
  platform?: string,
): Promise<LatestRelease | null> {
  const headers = { Accept: "application/vnd.github+json" };
  const stamp = Date.now();

  let data: GithubRelease | null = null;
  try {
    const res = await fetch(`${API}/releases/latest?ts=${stamp}`, {
      headers,
      cache: "no-store",
      signal: signalAfter(8000),
    });
    if (res.ok) data = (await res.json()) as GithubRelease;
    else if (res.status !== 404) throw new Error(`GitHub API 返回 HTTP ${res.status}`);
  } catch (e) {
    // 网络/代理错误：尝试列表接口，仍失败则抛出
    const res = await fetch(`${API}/releases?per_page=1&ts=${stamp}`, {
      headers,
      cache: "no-store",
      signal: signalAfter(8000),
    });
    if (!res.ok) throw e;
    const list = (await res.json()) as GithubRelease[];
    data = Array.isArray(list) ? (list[0] ?? null) : null;
  }

  if (!data) {
    const res = await fetch(`${API}/releases?per_page=1&ts=${stamp}`, {
      headers,
      cache: "no-store",
      signal: signalAfter(8000),
    });
    if (res.ok) {
      const list = (await res.json()) as GithubRelease[];
      data = Array.isArray(list) ? (list[0] ?? null) : null;
    } else if (res.status !== 404) {
      throw new Error(`GitHub API 返回 HTTP ${res.status}`);
    }
  }

  if (!data || data.draft) return null;

  const version = String(data.tag_name ?? "").replace(/^v/i, "");
  if (!version) return null;

  const assets: GithubAsset[] = Array.isArray(data.assets) ? data.assets : [];
  let asset: GithubAsset | undefined;
  if (platform === "windows") {
    asset =
      assets.find((a) => /setup\.exe$/i.test(a.name ?? "")) ??
      assets.find((a) => /\.exe$/i.test(a.name ?? ""));
  } else if (platform === "macos") {
    asset = assets.find((a) => /\.dmg$/i.test(a.name ?? ""));
  }

  const pageUrl = String(data.html_url ?? `https://github.com/${REPO}/releases`);
  return {
    version,
    pageUrl,
    downloadUrl: asset?.browser_download_url ?? pageUrl,
    fileName: asset?.name ?? null,
  };
}
