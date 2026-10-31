//! aginx-api — D17 api 腿运行时。
//!
//! 文件系统=注册表：扫 `channels/<kind>/leg/api-leg.toml` 载入 `[[tool]]`。
//! 凭据经账户选择器（`accounts/<id>/account.toml` → secretd scope）取得；
//! token 腿内缓存，永不进日志/stdout（验收面只露前缀）。

pub mod account;
pub mod leg;
pub mod policy;
pub mod resolve;
pub mod runtime;
pub mod secret_fetch;
pub mod token_cache;

pub use account::{resolve_account, AccountCreds, AccountMeta};
pub use leg::{load_leg, scan_legs, ApiLeg, ApiToolDef, ResolveDef};
pub use policy::{check_admin_gate, load_channel_policy, ChannelPolicy};
pub use runtime::{run_tool, RunOptions, RunResult};
pub use secret_fetch::fetch_secret_json;

/// `/home/channels`（`AGINX_HOME` 下，默认 `/home`）。
pub fn channels_root() -> std::path::PathBuf {
    let home = std::env::var_os("AGINX_HOME").unwrap_or_else(|| std::ffi::OsString::from("/home"));
    std::path::PathBuf::from(home).join("channels")
}
