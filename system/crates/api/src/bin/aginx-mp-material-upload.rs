//! aginx-mp-material-upload — 微信永久素材 multipart 上传（D17 cli 降级件）。
//!
//! `aginx-mp-material-upload --account <号> [--type image] <file>`
//! stdout JSON：`media_id` / `url`；完整 token 不打印。

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use aginx_api::account::resolve_account;
use aginx_api::runtime::redact_prefix;
use aginx_api::token_cache;
use clap::Parser;
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "aginx-mp-material-upload")]
struct Cli {
    #[arg(long, default_value = "weixin-oa")]
    channel: String,
    #[arg(long)]
    account: Option<String>,
    #[arg(long = "type", default_value = "image")]
    media_type: String,
    /// 打印 token 前缀到 stderr（调试）
    #[arg(long, default_value_t = false)]
    verbose: bool,
    file: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if !cli.file.is_file() {
        return fail("usage", &format!("not a file: {}", cli.file.display()));
    }

    let creds = match resolve_account(&cli.channel, cli.account.as_deref()) {
        Ok(c) => c,
        Err(e) => return fail("state", &e.to_string()),
    };

    let token = match ensure_token(&cli.channel, &creds.scope, cli.account.as_deref()) {
        Ok(t) => t,
        Err(e) => return fail("state", &e.to_string()),
    };
    if cli.verbose {
        eprintln!("aginx-mp-material-upload: token {}", redact_prefix(&token));
    }

    let url = format!(
        "https://api.weixin.qq.com/cgi-bin/material/add_material?access_token={}&type={}",
        urlencoding::encode(&token),
        urlencoding::encode(&cli.media_type)
    );

    match multipart_upload(&url, &cli.file) {
        Ok(parsed) => {
            let errcode = parsed.get("errcode").and_then(|v| v.as_i64());
            if parsed.get("errcode").is_some() && errcode != Some(0) {
                // 40001 → 清缓存提示重试
                if errcode == Some(40001) {
                    token_cache::invalidate(&creds.scope);
                }
                let msg = parsed
                    .get("errmsg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("upload failed");
                println!(
                    "{}",
                    json!({
                        "ok": false,
                        "error": {"type":"state","code":"weixin_api_error","message": msg},
                        "data": {"errcode": errcode, "account": creds.meta.id}
                    })
                );
                return ExitCode::from(1);
            }
            println!(
                "{}",
                json!({
                    "ok": true,
                    "data": {
                        "media_id": parsed.get("media_id"),
                        "url": parsed.get("url"),
                        "account": creds.meta.id,
                        "type": cli.media_type,
                    }
                })
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail("io", &e.to_string()),
    }
}

fn ensure_token(
    channel: &str,
    scope: &str,
    account: Option<&str>,
) -> anyhow::Result<String> {
    if let Some(t) = token_cache::get_cached(scope) {
        return Ok(t);
    }
    let opts = aginx_api::runtime::RunOptions {
        channel: Some(channel.to_string()),
        account: account.map(str::to_string),
        params: Default::default(),
        admin: false,
        redact_token: false,
        resolve_depth: 0,
    };
    let r = aginx_api::runtime::run_tool("weixin_mp_token", &opts)?;
    if !r.ok {
        anyhow::bail!("token failed: {:?}", r.errmsg);
    }
    r.extracted
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no access_token in response"))
}

fn multipart_upload(url: &str, file: &PathBuf) -> anyhow::Result<Value> {
    let mut f = File::open(file)?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    let filename = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("media.bin");
    let boundary = format!("----aginxmp{}", std::process::id());
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"media\"; filename=\"{filename}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(120)))
        .build()
        .new_agent();

    let resp = agent
        .post(url)
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body[..])
        .map_err(|e| anyhow::anyhow!("upload HTTP: {e}"))?;
    let text = resp
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("read: {e}"))?;
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({"_raw": text})))
}

fn fail(code: &str, msg: &str) -> ExitCode {
    eprintln!(
        "{}",
        json!({"ok": false, "error": {"type": "state", "code": code, "message": msg}})
    );
    ExitCode::from(1)
}
