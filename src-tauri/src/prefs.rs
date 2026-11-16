//! 全局偏好设置（主题、默认 openconnect 路径），与 Profile 分开存储。

use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PrefsData {
    /// light | dark
    pub theme: String,
    /// 全局兜底 openconnect 路径（优先级低于单个 Profile 的覆盖值）
    pub openconnect_path: Option<String>,
    /// 日志面板渲染上限
    pub log_render_limit: usize,
}

impl Default for PrefsData {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            openconnect_path: None,
            log_render_limit: 3000,
        }
    }
}

pub struct Prefs {
    file: PathBuf,
    data: Mutex<PrefsData>,
}

impl Prefs {
    pub fn load(dir: PathBuf) -> Self {
        let file = dir.join("prefs.json");
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<PrefsData>(&s).ok())
            .unwrap_or_default();
        Self {
            file,
            data: Mutex::new(data),
        }
    }

    pub fn get(&self) -> PrefsData {
        self.data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set(&self, value: PrefsData) -> Result<PrefsData, String> {
        {
            let mut guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
            *guard = value.clone();
        }
        if let Some(parent) = self.file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&value)
            .map_err(|e| format!("序列化设置失败：{e}"))?;
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| settings_error(&e))?;
        std::fs::rename(&tmp, &self.file).map_err(|e| settings_error(&e))?;
        Ok(value)
    }
}

#[tauri::command]
pub fn prefs_get(state: tauri::State<'_, crate::commands::AppState>) -> PrefsData {
    state.prefs.get()
}

#[tauri::command]
pub fn prefs_set(
    state: tauri::State<'_, crate::commands::AppState>,
    prefs: PrefsData,
) -> Result<PrefsData, String> {
    state.prefs.set(prefs)
}

fn settings_error(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return format!(
            "没有写入设置的权限：{e}\n\
             配置目录可能由旧版本以管理员身份运行时创建，属主是 root。\n\
             在终端执行一次后重试：\n\
             sudo chown -R \"$(id -un)\" \"$HOME/Library/Application Support/io.github.nbsod.octauri\""
        );
    }
    format!("保存设置失败：{e}")
}
