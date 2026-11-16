//! 配置持久化（JSON）。
//!
//! 只持久化非敏感字段（网关、账号、参数）。
//! 密码一律不落盘，每次连接时由用户输入，经 stdin 传给 openconnect。

use crate::model::{now_ms, Profile};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "profiles.json";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct StoreFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    profiles: Vec<Profile>,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// 配置目录的可用性：能否写、是否属于其他用户（旧版以 root 运行的遗留）。
    ///
    /// 这种遗留会让「添加配置」直接失败、已有配置也读不出来，
    /// 但程序自身没有权限修复，只能引导用户执行一次 chown。
    pub fn dir_status(&self) -> serde_json::Value {
        let path = self.dir.display().to_string();
        let exists = self.dir.is_dir();
        let writable = if exists {
            let probe = self.dir.join(".octauri-write-test");
            match std::fs::write(&probe, b"x") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    true
                }
                Err(_) => false,
            }
        } else {
            std::fs::create_dir_all(&self.dir).is_ok()
        };
        #[cfg(unix)]
        let foreign_owner = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&self.dir)
                .map(|m| m.uid() != unsafe { libc::geteuid() })
                .unwrap_or(false)
        };
        #[cfg(not(unix))]
        let foreign_owner = false;
        serde_json::json!({
            "path": path,
            "writable": writable,
            "foreignOwner": foreign_owner,
        })
    }

    fn file(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    /// 读取全部配置。文件缺失或损坏时返回空列表并保留损坏文件副本，
    /// 避免用户配置被静默清空。
    pub fn load(&self) -> Vec<Profile> {
        let path = self.file();
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        match serde_json::from_str::<StoreFile>(&raw) {
            Ok(f) => f.profiles,
            Err(e) => {
                let backup = self.dir.join(format!("profiles.corrupt-{}.json", now_ms()));
                let _ = std::fs::write(&backup, raw);
                eprintln!("[store] 配置文件解析失败({e})，已备份至 {}", backup.display());
                Vec::new()
            }
        }
    }

    /// 原子写入：先写临时文件再 rename，避免断电/崩溃导致文件截断。
    pub fn save_all(&self, profiles: &[Profile]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| write_error(&e, &self.dir))?;
        let file = StoreFile {
            version: 1,
            profiles: profiles.to_vec(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| format!("序列化配置失败：{e}"))?;
        let tmp = self.dir.join(format!("{FILE_NAME}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| write_error(&e, &self.dir))?;
        std::fs::rename(&tmp, self.file()).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            write_error(&e, &self.dir)
        })?;
        restrict_permissions(&self.file());
        Ok(())
    }

    pub fn upsert(&self, profile: Profile) -> Result<Profile, String> {
        let mut all = self.load();
        let mut p = profile.trimmed();
        validate(&p)?;
        let ts = now_ms();
        match all.iter_mut().find(|x| x.id == p.id) {
            Some(slot) => {
                p.created_at = slot.created_at;
                p.updated_at = ts;
                *slot = p.clone();
            }
            None => {
                if p.id.is_empty() {
                    p.id = new_id();
                }
                p.created_at = ts;
                p.updated_at = ts;
                all.push(p.clone());
            }
        }
        self.save_all(&all)?;
        Ok(p)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut all = self.load();
        let before = all.len();
        all.retain(|p| p.id != id);
        if all.len() == before {
            return Err("配置不存在".into());
        }
        self.save_all(&all)?;
        Ok(())
    }

    pub fn import(&self, json: &str) -> Result<usize, String> {
        let parsed: StoreFile = serde_json::from_str(json)
            .or_else(|_| {
                serde_json::from_str::<Vec<Profile>>(json)
                    .map(|profiles| StoreFile { version: 1, profiles })
            })
            .map_err(|e| format!("配置文件格式不正确：{e}"))?;
        if parsed.profiles.is_empty() {
            return Err("文件中没有任何配置".into());
        }
        let mut all = self.load();
        let mut count = 0usize;
        for mut p in parsed.profiles {
            let mut q = p.trimmed();
            if q.name.is_empty() || q.gateway.is_empty() {
                continue;
            }
            // id 冲突时重新分配，避免覆盖已有配置
            if q.id.is_empty() || all.iter().any(|x| x.id == q.id) {
                q.id = new_id();
            }
            let ts = now_ms();
            if q.created_at == 0 {
                q.created_at = ts;
            }
            q.updated_at = ts;
            p = q;
            all.push(p);
            count += 1;
        }
        if count == 0 {
            return Err("文件中的配置缺少名称或网关地址，已全部跳过".into());
        }
        self.save_all(&all)?;
        Ok(count)
    }

    pub fn export(&self, ids: &[String]) -> Result<String, String> {
        let all = self.load();
        let picked: Vec<Profile> = if ids.is_empty() {
            all
        } else {
            all.into_iter().filter(|p| ids.contains(&p.id)).collect()
        };
        if picked.is_empty() {
            return Err("没有可导出的配置".into());
        }
        let file = StoreFile {
            version: 1,
            profiles: picked,
        };
        serde_json::to_string_pretty(&file).map_err(|e| format!("序列化失败：{e}"))
    }
}

fn validate(p: &Profile) -> Result<(), String> {
    if p.name.is_empty() {
        return Err("配置名称不能为空".into());
    }
    if p.gateway.is_empty() {
        return Err("网关地址不能为空".into());
    }
    if p.gateway.contains(char::is_whitespace) {
        return Err("网关地址不能包含空格".into());
    }
    if p.auth_method == crate::model::AuthMethod::Certificate {
        if p.certificate_path.is_empty() {
            return Err("证书认证模式下必须指定客户端证书路径".into());
        }
        if !Path::new(&p.certificate_path).is_file() {
            return Err(format!("客户端证书不存在：{}", p.certificate_path));
        }
        if !p.key_path.is_empty() && !Path::new(&p.key_path).is_file() {
            return Err(format!("私钥文件不存在：{}", p.key_path));
        }
    }
    Ok(())
}

fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

pub fn new_id() -> String {
    // 不引入 uuid 依赖：时间戳 + 进程内计数 + 随机数足够本地唯一。
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("p{:x}{:x}", nanos, n)
}

/// 写入失败时的可操作提示。
///
/// 典型场景：早期版本以 root 运行，把配置目录建成了 root 所有，
/// 之后 GUI 改回普通用户就再也写不进去（`Permission denied`）。
/// 这种问题程序自身无法修复（需要更高的权限），必须明确告诉用户怎么做。
fn write_error(e: &std::io::Error, dir: &std::path::Path) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return format!(
            "没有写入配置的权限：{e}\n\
             该目录可能由旧版本以管理员身份运行时创建，属主是 root。\n\
             在终端执行一次后重试：\n\
             sudo chown -R \"$(id -un)\" \"{}\"",
            dir.display()
        );
    }
    format!("写入配置失败：{e}")
}
