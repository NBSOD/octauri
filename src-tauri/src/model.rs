//! 数据模型：Profile 配置、运行状态、日志行、环境探测报告。
//!
//! 说明：本程序为 openconnect 的图形外壳，所有隧道能力均由外部
//! openconnect 可执行文件提供，此处不含任何协议栈实现。

use serde::{Deserialize, Serialize};

/// 日志滚动缓冲上限（后端保留的条数）。前端另有渲染上限。
pub const LOG_BUFFER_CAP: usize = 2000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    /// 用户名 + 密码（--passwd-on-stdin）
    #[default]
    Password,
    /// 客户端证书 + 私钥（--certificate / --sslkey）
    Certificate,
}

/// 一个连接配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// 网关地址，例如 vpn.example.com 或 https://vpn.example.com:443
    pub gateway: String,
    pub username: String,
    #[serde(default)]
    pub auth_method: AuthMethod,
    /// AnyConnect / Pulse / GlobalProtect / NC 等，留空为 openconnect 自动探测
    #[serde(default)]
    pub protocol: String,
    /// 证书认证：客户端证书路径 (PEM)
    #[serde(default)]
    pub certificate_path: String,
    /// 证书认证：私钥路径 (PEM)
    #[serde(default)]
    pub key_path: String,
    /// 追加给 openconnect 的自定义参数，例如 "-v --no-dtls"
    #[serde(default)]
    pub extra_args: String,
    /// 该配置专属的 openconnect 可执行文件路径，留空则走全局/自动探测
    #[serde(default)]
    pub openconnect_path: String,
    /// 固定信任的服务器证书指纹（形如 pin-sha256:xxx）。
    /// 为空表示走正常 CA 校验；企业自签证书场景下由用户确认后写入。
    #[serde(default)]
    pub servercert: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl Profile {
    pub fn trimmed(&self) -> Profile {
        let mut p = self.clone();
        p.id = p.id.trim().to_string();
        p.name = p.name.trim().to_string();
        p.gateway = p.gateway.trim().to_string();
        p.username = p.username.trim().to_string();
        p.protocol = p.protocol.trim().to_string();
        p.certificate_path = p.certificate_path.trim().to_string();
        p.key_path = p.key_path.trim().to_string();
        p.extra_args = p.extra_args.trim().to_string();
        p.openconnect_path = p.openconnect_path.trim().to_string();
        p.servercert = p.servercert.trim().to_string();
        p
    }
}

/// 连接状态机：idle → handshaking → connected → error/idle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnState {
    Idle,
    Handshaking,
    Connected,
    Disconnecting,
    Error,
}

impl ConnState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnState::Idle => "idle",
            ConnState::Handshaking => "handshaking",
            ConnState::Connected => "connected",
            ConnState::Disconnecting => "disconnecting",
            ConnState::Error => "error",
        }
    }
}

/// 推送给前端的状态快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub state: String,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub pid: Option<u32>,
    /// 状态进入时间（毫秒时间戳）
    pub since: i64,
    pub message: String,
}

impl Default for StateSnapshot {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            profile_id: None,
            profile_name: None,
            pid: None,
            since: now_ms(),
            message: String::new(),
        }
    }
}

/// 一条日志。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub seq: u64,
    pub ts: i64,
    /// stdout | stderr | app
    pub stream: String,
    /// info | warn | error | success
    pub level: String,
    pub text: String,
    /// 故障标签：cert | handshake | dtls | auth | route | dns
    pub tag: Option<String>,
}

/// 环境探测结果（跨平台差异集中在 probe.rs）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvReport {
    pub platform: String,
    pub openconnect_path: Option<String>,
    pub openconnect_version: Option<String>,
    /// 探测到的所有候选路径，供用户在设置里挑选
    pub candidates: Vec<String>,
    /// 当前进程是否具备 root/管理员权限
    pub elevated: bool,
    /// 平台相关的权限指引文案
    pub elevation_hint: String,
    /// TUN/TAP 是否可用
    pub tun_ready: bool,
    pub tun_detail: String,
    /// 仅 Windows 有意义：wintun.dll 是否就绪
    pub wintun_ready: Option<bool>,
    pub wintun_detail: String,
    /// 阻断性问题（前端以横幅展示）
    pub warnings: Vec<String>,
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
/// 服务器证书不受信任时推给前端，供用户决定是否固定该证书。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertPrompt {
    pub profile_id: String,
    /// 验证失败原因，例如 "signer not found" / "self signed certificate"
    pub reason: String,
    /// 形如 pin-sha256:AbCd...
    pub pin: String,
}
