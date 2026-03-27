use crate::models::ServiceStatus;
use crate::utils::{platform, shell};
use tauri::command;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;
use log::{info, warn, debug};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows CREATE_NO_WINDOW 标志，用于隐藏控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const SERVICE_PORT: u16 = 18789;

/// 检测端口是否有服务在监听，返回 PID
/// 简单直接：端口被占用 = 服务运行中
fn check_port_listening(port: u16) -> Option<u32> {
    #[cfg(unix)]
    {
        let output = Command::new("lsof")
            .args(["-ti", &format!(":{}", port)])
            .output()
            .ok()?;
        
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .and_then(|line| line.trim().parse::<u32>().ok())
        } else {
            None
        }
    }
    
    #[cfg(windows)]
    {
        let mut cmd = Command::new("netstat");
        cmd.args(["-ano"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        let output = cmd.output().ok()?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(&format!(":{}", port)) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            return Some(pid);
                        }
                    }
                }
            }
        }
        None
    }
}

#[cfg(windows)]
fn windows_process_working_set_mb(pid: u32) -> Option<f64> {
    let mut cmd = Command::new("tasklist");
    cmd.args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = shell::decode_cli_output_bytes(&out.stdout);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let parts: Vec<&str> = line.split("\",\"").collect();
    let last = parts.last()?.trim_end_matches('"');
    let mem = last.strip_suffix(" K")?;
    let kb: f64 = mem.replace(",", "").parse().ok()?;
    let mb = kb / 1024.0;
    Some((mb * 100.0).round() / 100.0)
}

/// 获取服务状态（简单版：直接检查端口占用）
#[command]
pub async fn get_service_status() -> Result<ServiceStatus, String> {
    // 简单直接：检查端口是否被占用
    let pid = check_port_listening(SERVICE_PORT);
    let running = pid.is_some();

    let memory_mb = pid.and_then(|p| {
        #[cfg(windows)]
        {
            windows_process_working_set_mb(p)
        }
        #[cfg(not(windows))]
        {
            let _ = p;
            None::<f64>
        }
    });

    Ok(ServiceStatus {
        running,
        pid,
        port: SERVICE_PORT,
        uptime_seconds: None,
        memory_mb,
        cpu_percent: None,
    })
}

/// 启动服务
#[command]
pub async fn start_service() -> Result<String, String> {
    info!("[服务] 启动服务...");
    
    // 检查是否已经运行
    let status = get_service_status().await?;
    if status.running {
        info!("[服务] 服务已在运行中");
        return Err("服务已在运行中".to_string());
    }
    
    // 检查 openclaw 命令是否存在
    let openclaw_path = shell::get_openclaw_path();
    if openclaw_path.is_none() {
        info!("[服务] 找不到 openclaw 命令");
        return Err("找不到 openclaw 命令，请先通过 npm install -g openclaw 安装".to_string());
    }
    info!("[服务] openclaw 路径: {:?}", openclaw_path);
    
    // 直接后台启动 gateway（不等待 doctor，避免阻塞）
    info!("[服务] 后台启动 gateway...");
    shell::spawn_openclaw_gateway()
        .map_err(|e| format!("启动服务失败: {}", e))?;
    
    // 轮询等待端口开始监听（最多 15 秒）
    info!("[服务] 等待端口 {} 开始监听...", SERVICE_PORT);
    for i in 1..=15 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Some(pid) = check_port_listening(SERVICE_PORT) {
            info!("[服务] ✓ 启动成功 ({}秒), PID: {}", i, pid);
            return Ok(format!("服务已启动，PID: {}", pid));
        }
        if i % 3 == 0 {
            debug!("[服务] 等待中... ({}秒)", i);
        }
    }
    
    info!("[服务] 等待超时，端口仍未监听");
    Err("服务启动超时（15秒），请检查 openclaw 日志".to_string())
}

/// 获取监听指定端口的所有 PID
fn get_pids_on_port(port: u16) -> Vec<u32> {
    #[cfg(unix)]
    {
        let output = Command::new("lsof")
            .args(["-ti", &format!(":{}", port)])
            .output();
        
        match output {
            Ok(out) if out.status.success() => {
                let set: BTreeSet<u32> = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|line| line.trim().parse::<u32>().ok())
                    .collect();
                set.into_iter().collect()
            }
            _ => vec![],
        }
    }
    
    #[cfg(windows)]
    {
        let mut cmd = Command::new("netstat");
        cmd.args(["-ano"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        match cmd.output() {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let set: BTreeSet<u32> = stdout
                    .lines()
                    .filter(|line| line.contains(&format!(":{}", port)) && line.contains("LISTENING"))
                    .filter_map(|line| line.split_whitespace().last())
                    .filter_map(|pid_str| pid_str.parse::<u32>().ok())
                    .collect();
                set.into_iter().collect()
            }
            _ => vec![],
        }
    }
}

/// 通过 PID 杀死进程
fn kill_process(pid: u32, force: bool) -> bool {
    info!("[服务] 杀死进程 PID: {}, force: {}", pid, force);
    
    #[cfg(unix)]
    {
        let signal = if force { "-9" } else { "-TERM" };
        Command::new("kill")
            .args([signal, &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        // /T：结束进程树（避免子进程仍占用端口）；/F：强制结束
        if force {
            cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        } else {
            cmd.args(["/T", "/PID", &pid.to_string()]);
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// 停止服务（通过杀死监听端口的进程）
#[command]
pub async fn stop_service() -> Result<String, String> {
    info!("[服务] 停止服务...");
    
    let pids = get_pids_on_port(SERVICE_PORT);
    if pids.is_empty() {
        info!("[服务] 端口 {} 无进程监听，服务未运行", SERVICE_PORT);
        return Ok("服务未在运行".to_string());
    }
    
    info!("[服务] 发现 {} 个进程监听端口 {}: {:?}", pids.len(), SERVICE_PORT, pids);
    
    // 第一步：优雅终止 (SIGTERM)
    for &pid in &pids {
        kill_process(pid, false);
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    
    // 检查是否已停止
    let remaining = get_pids_on_port(SERVICE_PORT);
    if remaining.is_empty() {
        info!("[服务] ✓ 已停止");
        return Ok("服务已停止".to_string());
    }
    
    // 第二步：强制终止 (SIGKILL)
    info!("[服务] 仍有 {} 个进程存活，强制终止...", remaining.len());
    for &pid in &remaining {
        kill_process(pid, true);
    }
    std::thread::sleep(std::time::Duration::from_secs(2));

    let mut still_running = get_pids_on_port(SERVICE_PORT);
    #[cfg(windows)]
    if !still_running.is_empty() {
        // taskkill 偶发未清干净（权限/进程树），再试 PowerShell Stop-Process
        for &pid in &still_running {
            let script = format!(
                "Stop-Process -Id {} -Force -ErrorAction SilentlyContinue",
                pid
            );
            let _ = shell::run_powershell_output(&script);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        still_running = get_pids_on_port(SERVICE_PORT);
    }

    if still_running.is_empty() {
        info!("[服务] ✓ 已强制停止");
        Ok("服务已停止".to_string())
    } else {
        Err(format!("无法停止服务，仍有进程: {:?}", still_running))
    }
}

/// 重启服务
#[command]
pub async fn restart_service() -> Result<String, String> {
    info!("[服务] 重启服务...");

    // 优先尝试 CLI 优雅停止（不阻塞结果）
    let _ = shell::run_openclaw(&["gateway", "stop"]);

    match stop_service().await {
        Ok(msg) => info!("[服务] {}", msg),
        Err(e) => warn!("[服务] stop_service: {}（将继续尝试启动）", e),
    }
    std::thread::sleep(std::time::Duration::from_secs(3));

    match start_service().await {
        Ok(s) => Ok(s),
        Err(e) if e.contains("已在运行") => {
            warn!("[服务] 启动报「已在运行」，再次强制停止后重试");
            let _ = shell::run_openclaw(&["gateway", "stop"]);
            let _ = stop_service().await;
            std::thread::sleep(std::time::Duration::from_secs(3));
            start_service().await
        }
        Err(e) => Err(e),
    }
}

/// 读取日志文件（兼容 GBK/UTF-8），取末尾至多 max_lines 行
fn read_tail_lines_from_file(path: &std::path::Path, max_lines: u32, label: &str) -> std::io::Result<Vec<String>> {
    let bytes = std::fs::read(path)?;
    let content = shell::decode_cli_output_bytes(&bytes);
    let mut lines: Vec<String> = content
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.is_empty())
        .map(|l| {
            if label.is_empty() {
                l.to_string()
            } else {
                format!("[{}] {}", label, l)
            }
        })
        .collect();
    let cap = max_lines as usize;
    if lines.len() > cap {
        let start = lines.len() - cap;
        lines = lines.split_off(start);
    }
    Ok(lines)
}

/// 获取日志（扫描配置目录下 logs/*.log，并兼容 OPENCLAW_HOME）
#[command]
pub async fn get_logs(lines: Option<u32>) -> Result<Vec<String>, String> {
    let n = lines.unwrap_or(100);
    let mut bases: Vec<PathBuf> = Vec::new();
    bases.push(PathBuf::from(platform::get_config_dir()));
    if let Ok(extra) = std::env::var("OPENCLAW_HOME") {
        let t = extra.trim();
        if !t.is_empty() {
            let p = PathBuf::from(t);
            if p != bases[0] {
                bases.push(p);
            }
        }
    }

    let mut log_paths: Vec<PathBuf> = Vec::new();
    for base in &bases {
        let preferred = [
            base.join("logs").join("gateway.log"),
            base.join("logs").join("gateway.err.log"),
            base.join("stderr.log"),
            base.join("stdout.log"),
        ];
        for p in preferred {
            if p.is_file() {
                log_paths.push(p);
            }
        }

        let logs_dir = base.join("logs");
        if logs_dir.is_dir() {
            let mut extras: Vec<PathBuf> = std::fs::read_dir(&logs_dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if !p.is_file() {
                        return None;
                    }
                    let is_log = p
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("log"))
                        .unwrap_or(false);
                    is_log.then_some(p)
                })
                .collect();
            extras.sort();
            for p in extras {
                if !log_paths.contains(&p) {
                    log_paths.push(p);
                }
            }
        }
    }

    let multi_file = log_paths.len() > 1;
    let mut combined: Vec<String> = Vec::new();
    for path in log_paths {
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("log");
        let label = if multi_file { label } else { "" };
        match read_tail_lines_from_file(&path, n, label) {
            Ok(mut chunk) => combined.append(&mut chunk),
            Err(e) => debug!("读取 {:?} 失败: {}", path, e),
        }
    }

    if combined.len() > n as usize {
        combined = combined.split_off(combined.len() - n as usize);
    }

    Ok(combined)
}
