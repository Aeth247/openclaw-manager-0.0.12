use std::process::{Command, Output};
use std::io;
use std::collections::HashMap;
use crate::utils::platform;
use crate::utils::file;
use log::{info, debug, warn};

#[cfg(windows)]
use serde_json::Value as JsonValue;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows CREATE_NO_WINDOW 标志，用于隐藏控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 默认的 Gateway Token
pub const DEFAULT_GATEWAY_TOKEN: &str = "openclaw-manager-local-token";

/// 解码 CLI 子进程输出。中文 Windows 下控制台多为 GBK，直接用 UTF-8 会得到乱码或问号。
pub fn decode_cli_output_bytes(bytes: &[u8]) -> String {
    #[cfg(windows)]
    {
        use encoding_rs::GBK;
        if std::str::from_utf8(bytes).is_ok() {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let (cow, _, had_errors) = GBK.decode(bytes);
        if had_errors {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            cow.into_owned()
        }
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn merge_cli_stdout_stderr(stdout: &str, stderr: &str) -> String {
    let a = stdout.trim();
    let b = stderr.trim();
    match (a.is_empty(), b.is_empty()) {
        (false, false) => format!("{}\n{}", a, b),
        (false, _) => a.to_string(),
        (_, false) => b.to_string(),
        _ => String::new(),
    }
}

/// 从 `~/.npmrc`（及相同语法的用户级配置）解析 `prefix=...`，用于 GUI 进程未继承 shell PATH 时的全局包路径。
fn parse_npmrc_prefix_lines(content: &str) -> Vec<String> {
    let mut v = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let rest = line.strip_prefix("prefix").map(str::trim_start);
        let Some(rest) = rest else {
            continue;
        };
        let rest = match rest.strip_prefix('=') {
            Some(r) => r.trim(),
            None => continue,
        };
        let val = rest.trim_matches('"').trim_matches('\'');
        if !val.is_empty() {
            v.push(val.to_string());
        }
    }
    v
}

fn npm_global_prefixes_from_user_npmrc() -> Vec<String> {
    let mut v = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let np = home.join(".npmrc");
        if let Ok(content) = std::fs::read_to_string(&np) {
            v.extend(parse_npmrc_prefix_lines(&content));
        }
    }
    v
}

#[cfg(windows)]
fn windows_npm_global_prefix_dirs() -> Vec<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    let mut push = |s: String| {
        let s = normalize_cmd_path(s.trim());
        if s.is_empty() {
            return;
        }
        if seen.insert(s.clone()) {
            out.push(s);
        }
    };
    if let Ok(p) = std::env::var("npm_config_prefix") {
        push(p);
    }
    for p in npm_global_prefixes_from_user_npmrc() {
        push(p);
    }
    out
}

/// 获取扩展的 PATH 环境变量
/// GUI 应用启动时可能没有继承用户 shell 的 PATH，需要手动添加常见路径
pub fn get_extended_path() -> String {
    let mut paths = Vec::new();

    #[cfg(windows)]
    {
        // Windows 必须使用 `;` 连接 PATH；误用 `:` 会导致子进程找不到 node/openclaw
        paths.push(r"C:\Program Files\nodejs".to_string());
        paths.push(r"C:\Program Files (x86)\nodejs".to_string());
        paths.push("C:\\nvm4w\\nodejs".to_string());
        if let Ok(pf) = std::env::var("ProgramFiles") {
            paths.push(format!("{}\\nodejs", pf));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            paths.push(format!("{}\\nodejs", pf86));
        }
        // 自定义 npm 全局目录：环境变量 + 用户 ~/.npmrc 中的 prefix=
        for pref in windows_npm_global_prefix_dirs() {
            paths.push(pref.clone());
            paths.push(format!("{}\\bin", pref));
        }
        if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
            paths.push(nvm_symlink);
        }
        if let Ok(nvm_home) = std::env::var("NVM_HOME") {
            let settings_path = format!("{}\\settings.txt", nvm_home);
            if let Ok(content) = std::fs::read_to_string(&settings_path) {
                for line in content.lines() {
                    if line.starts_with("current:") {
                        if let Some(version) = line.strip_prefix("current:") {
                            let version = version.trim();
                            if !version.is_empty() {
                                paths.push(format!("{}\\v{}", nvm_home, version));
                            }
                        }
                        break;
                    }
                }
            }
        }
        if let Some(home) = dirs::home_dir() {
            let h = home.display().to_string();
            paths.push(format!("{}\\AppData\\Roaming\\npm", h));
            paths.push(format!("{}\\AppData\\Roaming\\nvm\\current", h));
            paths.push(format!("{}\\AppData\\Roaming\\fnm\\aliases\\default", h));
            paths.push(format!("{}\\AppData\\Local\\fnm\\aliases\\default", h));
            paths.push(format!("{}\\.fnm\\aliases\\default", h));
            paths.push(format!("{}\\scoop\\apps\\nodejs\\current", h));
            paths.push(format!("{}\\scoop\\apps\\nodejs-lts\\current", h));
        }
        paths.push("C:\\ProgramData\\chocolatey\\lib\\nodejs\\tools".to_string());
    }

    #[cfg(not(windows))]
    {
        // 用户 ~/.npmrc 中的 prefix=（先于系统路径尝试）
        for pref in npm_global_prefixes_from_user_npmrc().into_iter().rev() {
            let pref = pref.trim();
            if pref.is_empty() {
                continue;
            }
            paths.insert(0, format!("{}/bin", pref));
            paths.insert(0, pref.to_string());
        }
        paths.push("/opt/homebrew/bin".to_string()); // Homebrew on Apple Silicon
        paths.push("/usr/local/bin".to_string());
        paths.push("/usr/bin".to_string());
        paths.push("/bin".to_string());

        if let Some(home) = dirs::home_dir() {
            let home_str = home.display().to_string();

            let nvm_default = format!("{}/.nvm/alias/default", home_str);
            if let Ok(version) = std::fs::read_to_string(&nvm_default) {
                let version = version.trim();
                if !version.is_empty() {
                    paths.insert(0, format!("{}/.nvm/versions/node/v{}/bin", home_str, version));
                }
            }
            let nvm_versions_dir = std::path::Path::new(&home).join(".nvm/versions/node");
            if let Ok(entries) = std::fs::read_dir(&nvm_versions_dir) {
                let mut bins = Vec::new();
                for entry in entries.flatten() {
                    let nvm_bin = entry.path().join("bin");
                    if nvm_bin.exists() {
                        bins.push(nvm_bin.display().to_string());
                    }
                }
                bins.sort();
                bins.reverse();
                for bin in bins {
                    paths.push(bin);
                }
            }

            for version in ["v22.22.0", "v22.12.0", "v22.11.0", "v22.0.0", "v23.0.0"] {
                paths.push(format!("{}/.nvm/versions/node/{}/bin", home_str, version));
            }

            paths.push(format!("{}/.fnm/aliases/default/bin", home_str));
            paths.push(format!("{}/.volta/bin", home_str));
            paths.push(format!("{}/.asdf/shims", home_str));
            paths.push(format!("{}/.local/share/mise/shims", home_str));
        }
    }

    let current_path = std::env::var("PATH").unwrap_or_default();
    if !current_path.is_empty() {
        paths.push(current_path);
    }

    #[cfg(windows)]
    return paths.join(";");

    #[cfg(not(windows))]
    return paths.join(":");
}

/// 执行 Shell 命令（带扩展 PATH）
pub fn run_command(cmd: &str, args: &[&str]) -> io::Result<Output> {
    let mut command = Command::new(cmd);
    command.args(args);
    command.env("PATH", get_extended_path());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.output()
}

/// 执行 Shell 命令并获取输出字符串
pub fn run_command_output(cmd: &str, args: &[&str]) -> Result<String, String> {
    match run_command(cmd, args) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 执行 Bash 命令（带扩展 PATH）
pub fn run_bash(script: &str) -> io::Result<Output> {
    let mut command = Command::new("bash");
    command.arg("-c").arg(script);
    
    // 在非 Windows 系统上使用扩展的 PATH
    #[cfg(not(windows))]
    {
        let extended_path = get_extended_path();
        command.env("PATH", extended_path);
    }
    
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    
    command.output()
}

/// 执行 Bash 命令并获取输出
pub fn run_bash_output(script: &str) -> Result<String, String> {
    match run_bash(script) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    Err(format!("Command failed with exit code: {:?}", output.status.code()))
                } else {
                    Err(stderr)
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 执行 cmd.exe 命令（Windows）- 避免 PowerShell 执行策略问题
pub fn run_cmd(script: &str) -> io::Result<Output> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", script]);
    #[cfg(windows)]
    {
        cmd.env("PATH", get_extended_path());
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
}

/// 执行 cmd.exe 命令并获取输出（Windows）
pub fn run_cmd_output(script: &str) -> Result<String, String> {
    match run_cmd(script) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if stdout.is_empty() {
                        Err(format!("Command failed with exit code: {:?}", output.status.code()))
                    } else {
                        Err(stdout)
                    }
                } else {
                    Err(stderr)
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 执行 PowerShell 命令（Windows）- 仅在需要 PowerShell 特定功能时使用
/// 注意：某些 Windows 系统的 PowerShell 执行策略可能禁止运行脚本
pub fn run_powershell(script: &str) -> io::Result<Output> {
    let mut cmd = Command::new("powershell");
    // 使用 -ExecutionPolicy Bypass 绕过执行策略限制
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
    #[cfg(windows)]
    {
        cmd.env("PATH", get_extended_path());
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
}

/// 执行 PowerShell 命令并获取输出（Windows）
pub fn run_powershell_output(script: &str) -> Result<String, String> {
    match run_powershell(script) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if stdout.is_empty() {
                        Err(format!("Command failed with exit code: {:?}", output.status.code()))
                    } else {
                        Err(stdout)
                    }
                } else {
                    Err(stderr)
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 跨平台执行脚本命令
/// Windows 上使用 cmd.exe（避免 PowerShell 执行策略问题）
pub fn run_script_output(script: &str) -> Result<String, String> {
    if platform::is_windows() {
        run_cmd_output(script)
    } else {
        run_bash_output(script)
    }
}

/// 后台执行命令（不等待结果）
pub fn spawn_background(script: &str) -> io::Result<()> {
    if platform::is_windows() {
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", script]);
        
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        cmd.spawn()?;
    } else {
        Command::new("bash")
            .arg("-c")
            .arg(script)
            .spawn()?;
    }
    Ok(())
}

/// Windows：用与启动子进程相同的 PATH 解析 `where openclaw`，得到完整 .cmd 路径
#[cfg(windows)]
fn resolve_openclaw_via_where() -> Option<String> {
    let extended = get_extended_path();
    let mut cmd = Command::new("cmd");
    cmd.args(["/d", "/c", "where openclaw"]);
    cmd.env("PATH", &extended);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let out = decode_cli_output_bytes(&output.stdout);
    let mut candidates: Vec<String> = out
        .lines()
        .map(|l| normalize_cmd_path(l))
        .filter(|l| !l.is_empty())
        .collect();
    candidates.sort_by_key(|p| {
        let pl = p.to_lowercase();
        if pl.ends_with(".cmd") {
            0
        } else if pl.ends_with(".ps1") {
            2
        } else {
            1
        }
    });
    for p in candidates {
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    None
}

/// 规范化 CLI 路径（去除外层引号与空白，避免拼进 cmd 后变成非法「命令名」）
pub fn normalize_cmd_path(p: &str) -> String {
    p.trim_matches(|c| c == '"' || c == ' ' || c == '\t').to_string()
}

/// Windows：npm 全局 `openclaw.cmd` 与 `node_modules/openclaw` 同在前缀目录下。
/// 优先用 `node <package.bin>` 启动，避免 `.cmd` shim / `cmd` 引号解析导致无法执行。
#[cfg(windows)]
fn resolve_openclaw_entry_js_from_npm_global_cmd(cmd_path: &str) -> Option<std::path::PathBuf> {
    let cmd_path = normalize_cmd_path(cmd_path);
    let path = std::path::Path::new(&cmd_path);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "cmd" && ext != "bat" {
        return None;
    }
    let parent = path.parent()?;
    let pkg_root = parent.join("node_modules").join("openclaw");
    let pj_path = pkg_root.join("package.json");
    let pj_text = std::fs::read_to_string(&pj_path).ok()?;
    let v: JsonValue = serde_json::from_str(&pj_text).ok()?;
    let bin = v.get("bin")?;
    let rel = match bin {
        JsonValue::String(s) => s.clone(),
        JsonValue::Object(o) => o
            .get("openclaw")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .or_else(|| o.values().find_map(|x| x.as_str().map(|s| s.to_string())))?,
        _ => return None,
    };
    let rel = rel.trim_start_matches("./");
    let script = pkg_root.join(rel);
    if script.is_file() {
        Some(script)
    } else {
        None
    }
}

/// Windows：构造 `cmd /d /c` 后面的命令行。
///
/// **不要使用 `/s`**：`/s` 会额外剥掉一层引号，常把 `"D:\...\openclaw.cmd"` 拆坏，
/// 从而出现「'\"D:\...\openclaw.cmd\"' 不是内部或外部命令」。
/// 对 `.cmd`/`.bat` 使用 `call "路径" ...`，与手动在 cmd 里执行 npm 全局 shim 行为一致。
#[cfg(windows)]
fn windows_openclaw_cmd_script(openclaw_path: &str, args: &[&str]) -> String {
    let p_clean = normalize_cmd_path(openclaw_path);
    let mut rest = String::new();
    for a in args {
        rest.push(' ');
        if a.contains(' ') || a.contains('&') || a.contains('(') {
            let esc = a.replace('"', r#"\""#);
            rest.push('"');
            rest.push_str(&esc);
            rest.push('"');
        } else {
            rest.push_str(a);
        }
    }
    let use_call =
        ends_with_ignore_case(&p_clean, ".cmd") || ends_with_ignore_case(&p_clean, ".bat");
    if use_call {
        format!("call \"{}\"{}", p_clean.replace('"', ""), rest)
    } else {
        format!("\"{}\"{}", p_clean.replace('"', ""), rest)
    }
}

/// 获取 openclaw 可执行文件路径
/// 检测多个可能的安装路径，因为 GUI 应用不继承用户 shell 的 PATH
pub fn get_openclaw_path() -> Option<String> {
    // Windows: 检查常见的 npm 全局安装路径
    if platform::is_windows() {
        let possible_paths = get_windows_openclaw_paths();
        for path in possible_paths {
            if std::path::Path::new(&path).exists() {
                info!("[Shell] 在 {} 找到 openclaw", path);
                return Some(path);
            }
        }
    } else {
        // Unix: 检查常见的 npm 全局安装路径
        let possible_paths = get_unix_openclaw_paths();
        for path in possible_paths {
            if std::path::Path::new(&path).exists() {
                info!("[Shell] 在 {} 找到 openclaw", path);
                return Some(path);
            }
        }
    }
    
    // 回退：PATH 中有 openclaw 时，在 Windows 下必须解析出真实路径（裸用 Command::new("openclaw") 常触发 program not found）
    if command_exists("openclaw") {
        #[cfg(windows)]
        if let Some(p) = resolve_openclaw_via_where() {
            return Some(p);
        }
        return Some("openclaw".to_string());
    }
    
    // 最后尝试：通过用户 shell 查找
    if !platform::is_windows() {
        if let Ok(path) = run_bash_output("source ~/.zshrc 2>/dev/null || source ~/.bashrc 2>/dev/null; which openclaw 2>/dev/null") {
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                info!("[Shell] 通过用户 shell 找到 openclaw: {}", path);
                return Some(path);
            }
        }
    }

    // Windows：即使上面未命中，再用扩展 PATH + where（覆盖自定义 npm 前缀等）
    #[cfg(windows)]
    if let Some(p) = resolve_openclaw_via_where() {
        info!("[Shell] 扩展 PATH + where 找到 openclaw: {}", p);
        return Some(p);
    }

    None
}

/// 获取 Unix 系统上可能的 openclaw 安装路径
fn get_unix_openclaw_paths() -> Vec<String> {
    let mut paths = Vec::new();
    
    // npm 全局安装路径
    paths.push("/usr/local/bin/openclaw".to_string());
    paths.push("/opt/homebrew/bin/openclaw".to_string()); // Homebrew on Apple Silicon
    paths.push("/usr/bin/openclaw".to_string());
    
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        
        // npm 全局安装到用户目录
        paths.push(format!("{}/.npm-global/bin/openclaw", home_str));
        
        // nvm 安装的 npm 全局包：优先 default alias，再动态扫描已安装版本
        let nvm_default = format!("{}/.nvm/alias/default", home_str);
        if let Ok(version) = std::fs::read_to_string(&nvm_default) {
            let version = version.trim();
            if !version.is_empty() {
                let normalized = if version.starts_with('v') {
                    version.to_string()
                } else {
                    format!("v{}", version)
                };
                paths.push(format!("{}/.nvm/versions/node/{}/bin/openclaw", home_str, normalized));
            }
        }

        let nvm_versions_dir = std::path::Path::new(&home).join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_versions_dir) {
            let mut version_paths = Vec::new();
            for entry in entries.flatten() {
                let openclaw_path = entry.path().join("bin/openclaw");
                if openclaw_path.exists() {
                    version_paths.push(openclaw_path.display().to_string());
                }
            }
            version_paths.sort();
            version_paths.reverse();
            paths.extend(version_paths);
        }

        // 常见回退版本
        for version in ["v22.22.0", "v22.12.0", "v22.11.0", "v22.0.0", "v23.0.0"] {
            paths.push(format!("{}/.nvm/versions/node/{}/bin/openclaw", home_str, version));
        }
        
        // fnm
        paths.push(format!("{}/.fnm/aliases/default/bin/openclaw", home_str));
        
        // volta
        paths.push(format!("{}/.volta/bin/openclaw", home_str));
        
        // pnpm 全局安装
        paths.push(format!("{}/.pnpm/bin/openclaw", home_str));
        paths.push(format!("{}/Library/pnpm/openclaw", home_str)); // macOS pnpm 默认路径
        
        // asdf
        paths.push(format!("{}/.asdf/shims/openclaw", home_str));
        
        // mise (formerly rtx)
        paths.push(format!("{}/.local/share/mise/shims/openclaw", home_str));
        
        // yarn 全局安装
        paths.push(format!("{}/.yarn/bin/openclaw", home_str));
        paths.push(format!("{}/.config/yarn/global/node_modules/.bin/openclaw", home_str));
    }
    
    paths
}

/// 获取 Windows 上可能的 openclaw 安装路径
fn get_windows_openclaw_paths() -> Vec<String> {
    let mut paths = Vec::new();
    
    // 1. nvm4w 安装路径
    paths.push("C:\\nvm4w\\nodejs\\openclaw.cmd".to_string());
    
    // 2. 用户目录下的 npm 全局路径
    if let Some(home) = dirs::home_dir() {
        let h = home.display().to_string();
        paths.push(format!("{}\\AppData\\Roaming\\npm\\openclaw.cmd", h));
        paths.push(format!("{}\\AppData\\Local\\npm\\openclaw.cmd", h));
    }
    
    for pref in windows_npm_global_prefix_dirs() {
        paths.push(format!("{}\\openclaw.cmd", pref));
    }

    // 3. Program Files 下的 nodejs
    paths.push("C:\\Program Files\\nodejs\\openclaw.cmd".to_string());
    paths.push("C:\\Program Files (x86)\\nodejs\\openclaw.cmd".to_string());
    if let Ok(pf) = std::env::var("ProgramFiles") {
        paths.push(format!("{}\\nodejs\\openclaw.cmd", pf));
    }
    if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
        paths.push(format!("{}\\openclaw.cmd", nvm_symlink));
    }
    if let Ok(nvm_home) = std::env::var("NVM_HOME") {
        let settings_path = format!("{}\\settings.txt", nvm_home);
        if let Ok(content) = std::fs::read_to_string(&settings_path) {
            for line in content.lines() {
                if line.starts_with("current:") {
                    if let Some(version) = line.strip_prefix("current:") {
                        let version = version.trim();
                        if !version.is_empty() {
                            paths.push(format!("{}\\v{}\\openclaw.cmd", nvm_home, version));
                        }
                    }
                    break;
                }
            }
        }
    }
    
    paths
}

/// 从 openclaw --version 等输出的 stdout/stderr 中提取版本（不少 Node CLI 把版本打在 stderr）
fn parse_openclaw_version_from_output(stdout: &str, stderr: &str) -> Option<String> {
    for chunk in [stdout, stderr] {
        for line in chunk.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let skip = line.contains("ExperimentalWarning")
                || line.contains("DeprecationWarning")
                || line.starts_with("(");
            if skip {
                continue;
            }
            // 整行即版本
            if let Some(v) = normalize_version_token(&clean_token(line)) {
                return Some(v);
            }
            // 行内最后一个类似 x.y.z 的 token
            for tok in line.split_whitespace().rev() {
                let t = clean_token(tok);
                if let Some(v) = normalize_version_token(&t) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn clean_token(s: &str) -> String {
    s.trim_matches(|c| c == '(' || c == ')' || c == ',' || c == '"' || c == '\'')
        .trim()
        .to_string()
}

/// 识别主版本号.次版本号 形式（含可选 v 前缀）
fn normalize_version_token(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let body = t.strip_prefix('v').unwrap_or(t);
    let mut parts = body.split('.');
    let _major = parts.next()?.parse::<u32>().ok()?;
    let _minor = parts.next()?.parse::<u32>().ok()?;
    Some(if t.starts_with('v') {
        t.to_string()
    } else {
        format!("v{}", body)
    })
}

/// 根据 CLI 路径推断全局安装的 `node_modules/openclaw` 目录（Windows 与常见 npm Unix 布局）。
fn openclaw_npm_package_roots(cli_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    let Some(parent) = cli_path.parent() else {
        return roots;
    };
    roots.push(parent.join("node_modules").join("openclaw"));
    if parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
        if let Some(prefix) = parent.parent() {
            roots.push(
                prefix
                    .join("lib")
                    .join("node_modules")
                    .join("openclaw"),
            );
        }
    }
    roots
}

/// 当 `openclaw --version` 无法执行时，从已安装包内的 package.json 读取版本（供界面展示）。
pub fn read_installed_openclaw_version_from_package_json() -> Option<String> {
    let raw = get_openclaw_path()?;
    let path_norm = normalize_cmd_path(&raw);
    let p = std::path::Path::new(&path_norm);
    for root in openclaw_npm_package_roots(p) {
        let pj = root.join("package.json");
        let Ok(txt) = std::fs::read_to_string(&pj) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) else {
            continue;
        };
        let Some(s) = val.get("version").and_then(|x| x.as_str()) else {
            continue;
        };
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        return Some(if s.starts_with('v') {
            s.to_string()
        } else {
            format!("v{}", s)
        });
    }
    None
}

/// 优先执行 CLI 取版本；失败则读 `node_modules/openclaw/package.json`。
pub fn get_openclaw_version_for_display() -> Option<String> {
    if get_openclaw_path().is_none() {
        return None;
    }
    match run_openclaw(&["--version"]) {
        Ok(v) => {
            let t = v.trim();
            if !t.is_empty() {
                Some(t.to_string())
            } else {
                read_installed_openclaw_version_from_package_json()
            }
        }
        Err(_) => read_installed_openclaw_version_from_package_json(),
    }
}

/// 执行 openclaw 命令并获取输出
pub fn run_openclaw(args: &[&str]) -> Result<String, String> {
    debug!("[Shell] 执行 openclaw 命令: {:?}", args);
    
    let openclaw_path = get_openclaw_path()
        .map(|s| normalize_cmd_path(&s))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            warn!("[Shell] 找不到 openclaw 命令");
            "找不到 openclaw 命令，请确保已通过 npm install -g openclaw 安装".to_string()
        })?;
    
    debug!("[Shell] openclaw 路径: {}", openclaw_path);
    
    // 获取扩展的 PATH，确保能找到 node
    let extended_path = get_extended_path();
    debug!("[Shell] 扩展 PATH: {}", extended_path);

    let output = {
        #[cfg(windows)]
        {
            if let Some(script) = resolve_openclaw_entry_js_from_npm_global_cmd(&openclaw_path) {
                debug!(
                    "[Shell] Windows 使用 node 直连: {}",
                    script.display()
                );
                let mut cmd = Command::new("node");
                cmd.arg(&script)
                    .args(args)
                    .env("OPENCLAW_GATEWAY_TOKEN", DEFAULT_GATEWAY_TOKEN)
                    .env("PATH", &extended_path);
                cmd.creation_flags(CREATE_NO_WINDOW);
                cmd.output()
            } else {
                let script = windows_openclaw_cmd_script(&openclaw_path, args);
                debug!("[Shell] Windows cmd /c: {}", script);
                let mut cmd = Command::new("cmd");
                cmd.args(["/d", "/c", &script])
                    .env("OPENCLAW_GATEWAY_TOKEN", DEFAULT_GATEWAY_TOKEN)
                    .env("PATH", &extended_path);
                cmd.creation_flags(CREATE_NO_WINDOW);
                cmd.output()
            }
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new(&openclaw_path);
            cmd.args(args)
                .env("OPENCLAW_GATEWAY_TOKEN", DEFAULT_GATEWAY_TOKEN)
                .env("PATH", &extended_path);
            cmd.output()
        }
    };
    
    match output {
        Ok(out) => {
            let stdout = decode_cli_output_bytes(&out.stdout);
            let stderr = decode_cli_output_bytes(&out.stderr);
            debug!("[Shell] 命令退出码: {:?}", out.status.code());

            // --version：即使 exit code 非 0，只要解析出版本即视为成功（部分环境 stderr 混杂警告）
            if args.first().copied() == Some("--version")
                || args.first().copied() == Some("-V")
            {
                if let Some(v) = parse_openclaw_version_from_output(&stdout, &stderr) {
                    return Ok(v);
                }
            }

            if out.status.success() {
                let text = merge_cli_stdout_stderr(&stdout, &stderr);
                debug!("[Shell] 命令执行成功, 输出长度: {}", text.len());
                Ok(text)
            } else {
                debug!("[Shell] 命令执行失败, stderr: {}", stderr);
                Err(merge_cli_stdout_stderr(&stdout, &stderr))
            }
        }
        Err(e) => {
            warn!("[Shell] 执行 openclaw 失败: {}", e);
            Err(format!("执行 openclaw 失败: {}", e))
        }
    }
}

#[cfg(windows)]
fn ends_with_ignore_case(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len()
        && s[s.len() - suffix.len()..]
            .eq_ignore_ascii_case(suffix)
}

#[cfg(not(windows))]
fn ends_with_ignore_case(s: &str, suffix: &str) -> bool {
    s.ends_with(suffix)
}

/// 从 ~/.openclaw/env 文件读取所有环境变量
/// 与 shell 脚本 `source ~/.openclaw/env` 行为一致
fn load_openclaw_env_vars() -> HashMap<String, String> {
    let mut env_vars = HashMap::new();
    let env_path = platform::get_env_file_path();
    
    if let Ok(content) = file::read_file(&env_path) {
        for line in content.lines() {
            let line = line.trim();
            // 跳过注释和空行
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // 解析 export KEY=VALUE 或 KEY=VALUE 格式
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                // 去除值周围的引号
                let value = value.trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                env_vars.insert(key.to_string(), value.to_string());
            }
        }
    }
    
    env_vars
}

/// 后台启动 openclaw gateway
/// 与 shell 脚本行为一致：先加载 env 文件，再启动 gateway
pub fn spawn_openclaw_gateway() -> io::Result<()> {
    info!("[Shell] 后台启动 openclaw gateway...");
    
    let openclaw_path = get_openclaw_path()
        .map(|s| normalize_cmd_path(&s))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            warn!("[Shell] 找不到 openclaw 命令");
            io::Error::new(
                io::ErrorKind::NotFound,
                "找不到 openclaw 命令，请确保已通过 npm install -g openclaw 安装"
            )
        })?;
    
    info!("[Shell] openclaw 路径: {}", openclaw_path);
    
    // 加载用户的 env 文件环境变量（与 shell 脚本 source ~/.openclaw/env 一致）
    info!("[Shell] 加载用户环境变量...");
    let user_env_vars = load_openclaw_env_vars();
    info!("[Shell] 已加载 {} 个环境变量", user_env_vars.len());
    for key in user_env_vars.keys() {
        debug!("[Shell] - 环境变量: {}", key);
    }
    
    // 获取扩展的 PATH，确保能找到 node
    let extended_path = get_extended_path();
    info!("[Shell] 扩展 PATH: {}", extended_path);
    
    #[cfg(windows)]
    let mut cmd = {
        if let Some(script) = resolve_openclaw_entry_js_from_npm_global_cmd(&openclaw_path) {
            info!(
                "[Shell] Windows 启动 gateway (node): {}",
                script.display()
            );
            let mut c = Command::new("node");
            c.arg(&script).args(["gateway", "--port", "18789"]);
            c
        } else {
            let script =
                windows_openclaw_cmd_script(&openclaw_path, &["gateway", "--port", "18789"]);
            info!("[Shell] Windows 启动 (cmd): {}", script);
            let mut c = Command::new("cmd");
            c.args(["/d", "/c", &script]);
            c
        }
    };
    #[cfg(not(windows))]
    let mut cmd = {
        info!("[Shell] Unix 模式: 直接执行");
        let mut c = Command::new(&openclaw_path);
        c.args(["gateway", "--port", "18789"]);
        c
    };
    
    // 注入用户的环境变量（如 ANTHROPIC_API_KEY, OPENAI_API_KEY 等）
    for (key, value) in &user_env_vars {
        cmd.env(key, value);
    }
    
    // 设置 PATH 和 gateway token
    cmd.env("PATH", &extended_path);
    cmd.env("OPENCLAW_GATEWAY_TOKEN", DEFAULT_GATEWAY_TOKEN);
    
    // Windows: 隐藏控制台窗口
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    
    let logs_dir = std::path::PathBuf::from(platform::get_config_dir()).join("logs");
    let _ = std::fs::create_dir_all(&logs_dir);

    let stdout_log_path = logs_dir.join("gateway.log");
    let stderr_log_path = logs_dir.join("gateway.err.log");
    
    info!(
        "[Shell] 日志输出到: {} / {}",
        stdout_log_path.display(),
        stderr_log_path.display()
    );

    if let Ok(stdout_file) = std::fs::OpenOptions::new()
        .create(true).append(true).open(&stdout_log_path)
    {
        cmd.stdout(std::process::Stdio::from(stdout_file));
    }
    if let Ok(stderr_file) = std::fs::OpenOptions::new()
        .create(true).append(true).open(&stderr_log_path)
    {
        cmd.stderr(std::process::Stdio::from(stderr_file));
    }
    
    info!("[Shell] 启动 gateway 进程...");
    let child = cmd.spawn();
    
    match child {
        Ok(c) => {
            info!("[Shell] ✓ Gateway 进程已启动, PID: {}", c.id());
            Ok(())
        }
        Err(e) => {
            warn!("[Shell] ✗ Gateway 启动失败: {}", e);
            Err(io::Error::new(
                e.kind(),
                format!("启动失败 (路径: {}): {}", openclaw_path, e)
            ))
        }
    }
}

/// 检查命令是否存在
///
/// 必须用 `#[cfg]` 分平台实现：若写成 `if platform::is_windows() { ... creation_flags ... }`，
/// 在 Linux/macOS 上仍会编译 Windows 分支，而 `creation_flags` / `CREATE_NO_WINDOW` 仅在 Windows 可用。
#[cfg(windows)]
pub fn command_exists(cmd: &str) -> bool {
    // Windows：where 必须带上与执行 openclaw 相同的 PATH，否则 GUI 进程常误判为不存在
    let mut command = Command::new("cmd");
        command
            .args(["/d", "/c", &format!("where {}", cmd)])
        .env("PATH", get_extended_path());
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
