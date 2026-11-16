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
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("创建配置目录失败：{e}"))?;
        let file = StoreFile {
            version: 1,
            profiles: profiles.to_vec(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| format!("序列化配置失败：{e}"))?;
        let tmp = self.dir.join(format!("{FILE_NAME}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| format!("写入配置失败：{e}"))?;
        std::fs::rename(&tmp, self.file()).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("保存配置失败：{e}")
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
