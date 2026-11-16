use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// 响应体读取上限：2GB。ureq 默认仅 10MB，安装包远超该值。
const 最大响应字节: u64 = 2 * 1024 * 1024 * 1024;

/// 网络行为参数（来自 全局配置 [网络] 段，环境变量可覆盖用于测试）
struct 网络参数 {
    超时秒: u64,
    重试次数: u32,
    代理: String,
    离线模式: bool,
}

/// 读取全局配置的网络段；环境变量优先级更高（集成测试/临时排障用）。
fn 读取网络参数() -> 网络参数 {
    let 配置 = pm::全局配置::读取配置();
    let mut 参数 = 网络参数 {
        超时秒: 配置.网络.超时 as u64,
        重试次数: 配置.网络.重试次数,
        代理: 配置.网络.代理.clone(),
        离线模式: 配置.网络.离线模式,
    };
    if let Ok(值) = std::env::var("智语言_网络_离线模式") {
        参数.离线模式 = matches!(值.trim(), "1" | "真" | "true" | "TRUE");
    }
    if let Ok(值) = std::env::var("智语言_网络_超时") {
        if let Ok(n) = 值.trim().parse::<u64>() {
            参数.超时秒 = n;
        }
    }
    if let Ok(值) = std::env::var("智语言_网络_重试次数") {
        if let Ok(n) = 值.trim().parse::<u32>() {
            参数.重试次数 = n;
        }
    }
    if 参数.超时秒 == 0 {
        参数.超时秒 = 30;
    }
    参数
}

/// 开发套件镜像源地址。
///
/// 优先级：环境变量 智语言_开发套件镜像源_URL / 智语言_开发套件_URL（测试/排障）
/// > 全局配置 [镜像源].开发套件镜像源 > 官方源常量。空值视为未设置。
pub fn 镜像源地址() -> String {
    for 键 in ["智语言_开发套件镜像源_URL", "智语言_开发套件_URL"] {
        if let Ok(环境变量) = std::env::var(键) {
            if !环境变量.trim().is_empty() {
                return 环境变量.trim().trim_end_matches('/').to_string();
            }
        }
    }
    // 开发套件镜像源地址() 已内置「空 = 官方源」回退
    pm::全局配置::读取配置()
        .开发套件镜像源地址()
        .trim_end_matches('/')
        .to_string()
}

/// 去除末尾斜杠；并把中文域名（IDN）主机名转为 Punycode。
///
/// 官方源是中文域名 https://智语言.org，HTTP 客户端按 RFC 3986 只接受 ASCII 主机名，
/// 直接发送会报 "invalid uri character"。此处统一做 IDNA ToASCII。
fn 获取源地址() -> String {
    let mut 地址 = 镜像源地址();
    while 地址.ends_with('/') {
        地址.pop();
    }
    pm::网址规范化::规范化网址(&地址)
}

/// 按 RFC 3986 对路径/查询片段做百分号编码
fn url编码(原文: &str) -> String {
    let mut 出 = String::with_capacity(原文.len() * 3);
    for b in 原文.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                出.push(b as char)
            }
            _ => 出.push_str(&format!("%{:02X}", b)),
        }
    }
    出
}

/// 统一远程请求（ureq 3.x：非 2xx 不视为错误，需手动检查状态码）
///
/// 统一网络策略（规范 6.1.9 [网络] 段驱动）：离线模式立即短路；超时/代理取自
/// 全局配置；仅连接层失败按 重试次数 线性退避重试，HTTP 4xx/5xx 不重试。
fn 远程请求(方法: &str, url: &str, 可选体: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
    let 参数 = 读取网络参数();
    if 参数.离线模式 {
        return Err(format!(
            "当前为离线模式，已跳过远程请求：{}。如需联网请执行「智语言 设置 离线模式 假」",
            url
        ));
    }
    let 最大尝试 = 参数.重试次数.max(1);
    let mut 最后错误 = String::new();
    for 第几次 in 1..=最大尝试 {
        match 单次远程请求(&参数, 方法, url, 可选体.clone()) {
            Ok(数据) => return Ok(数据),
            Err((可重试, 错误)) => {
                最后错误 = 错误;
                if !可重试 || 第几次 == 最大尝试 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(200 * 第几次 as u64));
            }
        }
    }
    Err(最后错误)
}

/// 单次请求。返回 Err((是否可重试, 错误信息))。
fn 单次远程请求(
    参数: &网络参数,
    方法: &str,
    url: &str,
    可选体: Option<Vec<u8>>,
) -> Result<Vec<u8>, (bool, String)> {
    // 超时取自配置；代理为空时显式禁用（避免 ureq 把本地 127.0.0.1 走代理导致挂起）
    let 连接超时 = std::time::Duration::from_secs(参数.超时秒.min(10).max(1));
    let 构建器 = ureq::Agent::config_builder()
        .timeout_connect(Some(连接超时))
        .timeout_global(Some(std::time::Duration::from_secs(参数.超时秒)));
    let 构建器 = match ureq::Proxy::new(参数.代理.trim()) {
        Ok(代理配置) if !参数.代理.trim().is_empty() => 构建器.proxy(Some(代理配置)),
        _ => 构建器.proxy(None),
    };
    let 代理 = 构建器.build().new_agent();
    let 响应 = match 方法 {
        "GET" => 代理.get(url).call(),
        "POST" => 代理.post(url).send(可选体.unwrap_or_default()),
        "DELETE" => 代理.delete(url).call(),
        _ => return Err((false, format!("不支持的 HTTP 方法: {}", 方法))),
    };
    match 响应 {
        Ok(mut 响应) => {
            let 状态 = 响应.status().as_u16();
            // 必须显式放开 ureq 默认 10MB 响应体上限：开发套件安装包动辄数十 MB，
            // 用默认 read_to_vec() 会直接报 "larger than request limit"，导致安装永远失败。
            let 缓冲 = {
                use std::io::Read;
                let mut 数据 = Vec::new();
                响应
                    .body_mut()
                    .with_config()
                    .limit(最大响应字节)
                    .reader()
                    .read_to_end(&mut 数据)
                    .map_err(|e| (true, format!("读取响应失败: {}", e)))?;
                数据
            };
            if 状态 >= 400 {
                return Err((
                    false,
                    format!("服务返回错误 ({}): {}", 状态, String::from_utf8_lossy(&缓冲)),
                ));
            }
            Ok(缓冲)
        }
        Err(e) => Err((true, format!("请求失败: {}", e))),
    }
}

/// 标识当前平台（与服务端下载路径的 :平台 段一致）
fn 平台标识() -> String {
    let 系统 = std::env::consts::OS;
    let 架构 = std::env::consts::ARCH;
    match (系统, 架构) {
        ("windows", "x86_64") => "windows-x64".to_string(),
        ("windows", "aarch64") => "windows-arm64".to_string(),
        ("linux", "x86_64") => "linux-x64".to_string(),
        ("linux", "aarch64") => "linux-arm64".to_string(),
        ("macos", "x86_64") => "macos-x64".to_string(),
        ("macos", "aarch64") => "macos-arm64".to_string(),
        _ => format!("{}-{}", 系统, 架构),
    }
}

/// 查询可安装版本号列表（稳定版或测试版）
pub fn 查询可安装版本(测试版: bool) -> Result<Vec<String>, String> {
    // 响应字段名是 稳定版/测试版，而服务端查询参数 类型 只认 "测试"（见 版本路由.js）
    let 字段名 = if 测试版 { "测试版" } else { "稳定版" };
    let 类型值 = if 测试版 { "测试" } else { "稳定" };
    // 路径段、查询参数名、参数值都要各自单独百分号编码（ureq 不会自动编码中文，
    // 且 '=' 是分隔符不能被一起编码，否则服务端解析不出参数）
    let url = format!(
        "{}/{}?{}={}",
        获取源地址(),
        url编码("版本"),
        url编码("类型"),
        url编码(类型值)
    );
    let 字节 = 远程请求("GET", &url, None)?;
    let 类型 = 字段名;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析版本列表失败: {}", e))?;
    let 列表 = 值
        .get(类型)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("版本列表缺少 {} 字段", 类型))?;
    let mut 版本 = Vec::new();
    for 项 in 列表 {
        if let Some(s) = 项.as_str() {
            版本.push(s.to_string());
        }
    }
    Ok(版本)
}

/// 单个版本的详细信息（GET /版本/{版本号}）
#[derive(Debug, Clone)]
pub struct 版本详情 {
    pub 版本号: String,
    pub 发布时间: String,
    pub 说明: String,
    pub 平台列表: Vec<String>,
}

/// 查询指定版本的详细信息
pub fn 查询版本详情(版本号: &str) -> Result<版本详情, String> {
    let url = format!(
        "{}/{}/{}",
        获取源地址(),
        url编码("版本"),
        url编码(版本号)
    );
    let 字节 = 远程请求("GET", &url, None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析版本信息失败: {}", e))?;
    let 取文本 = |键: &str| -> String {
        值.get(键)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    // 平台列表兼容两种服务端形态：
    //   ① 字符串数组      ["windows-x64", ...]
    //   ② 对象数组        [{"平台":"windows-x64","文件名":"...","大小":..,"校验和":".."}, ...]
    // 生产存储用形态 ②，若只取 as_str 会静默得到空列表，导致"该版本无可用平台"的误判。
    let 平台列表: Vec<String> = 值
        .get("平台列表")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|项| match 项 {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(_) => 项
                        .get("平台")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    // 发布时间/说明 兼容服务端两套键名：发布时间|发布日期、说明|更新内容。
    // 更新内容 为字符串数组时用「；」拼接，直接取 as_str 会得到空串。
    let 取文本多键 = |键组: &[&str]| -> String {
        for 键 in 键组 {
            match 值.get(*键) {
                Some(Value::String(s)) if !s.is_empty() => return s.clone(),
                Some(Value::Array(项目)) => {
                    let 合并: Vec<String> = 项目
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect();
                    if !合并.is_empty() {
                        return 合并.join("；");
                    }
                }
                _ => {}
            }
        }
        String::new()
    };
    Ok(版本详情 {
        版本号: {
            let v = 取文本("版本号");
            if v.is_empty() {
                版本号.to_string()
            } else {
                v
            }
        },
        发布时间: 取文本多键(&["发布时间", "发布日期"]),
        说明: 取文本多键(&["说明", "更新内容"]),
        平台列表,
    })
}

/// 下载指定版本的安装程序（Windows 为 .exe，其余为 .sh），返回本地文件路径
pub fn 下载安装程序(版本号: &str, 目标目录: &Path) -> Result<PathBuf, String> {
    let 平台 = 平台标识();
    let url = format!(
        "{}/{}/{}/{}/{}",
        获取源地址(),
        url编码("下载"),
        url编码(版本号),
        url编码(&平台),
        url编码("安装程序")
    );
    let 字节 = 远程请求("GET", &url, None)?;

    fs::create_dir_all(目标目录).map_err(|e| format!("创建目录失败: {}", e))?;
    let 后缀 = if cfg!(windows) { "exe" } else { "sh" };
    let 文件路径 = 目标目录.join(format!("智语言开发套件{}.{}", 版本号, 后缀));
    fs::write(&文件路径, &字节).map_err(|e| format!("写入安装程序失败: {}", e))?;
    Ok(文件路径)
}

/// 比较两个点分版本号（数值逐段比较，段数不同以 0 补齐）
fn 版本大于(甲: &str, 乙: &str) -> bool {
    let 解析 = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|x| {
                x.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let (a, b) = (解析(甲), 解析(乙));
    let 长 = a.len().max(b.len());
    for i in 0..长 {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// 把 "latest"/"最新" 解析为镜像源上真实的最高稳定版版本号；
/// 其余输入原样返回。服务端不存在名为 latest 的版本，必须先解析再下载。
pub fn 解析版本别名(版本号: &str) -> Result<String, String> {
    if 版本号 != "latest" && 版本号 != "最新" {
        return Ok(版本号.to_string());
    }
    let 列表 = 查询可安装版本(false)?;
    let mut 最高: Option<String> = None;
    for v in 列表 {
        最高 = match 最高 {
            None => Some(v),
            Some(旧) => {
                if 版本大于(&v, &旧) {
                    Some(v)
                } else {
                    Some(旧)
                }
            }
        };
    }
    最高.ok_or_else(|| "镜像源上没有可安装的稳定版".to_string())
}

/// 取镜像源上**最高测试版**版本号（供 `智语言 安装 --测试版` 使用）。
/// ⚠ 2026-10-03：`执行安装` 原本把 `--测试版` 与默认分支都写成 "latest"（`测试版.then_some("latest")
///   .unwrap_or("latest")`），该开关**完全无效** ⇒ `智语言 安装 --测试版` 实际装的是稳定版 latest，
///   与帮助文本「智语言 安装 --测试版」的承诺不符。现按测试版列表解析最高版本。
pub fn 最新测试版() -> Result<String, String> {
    let 列表 = 查询可安装版本(true)?;
    let mut 最高: Option<String> = None;
    for v in 列表 {
        最高 = match 最高 {
            None => Some(v),
            Some(旧) => Some(if 版本大于(&v, &旧) { v } else { 旧 }),
        };
    }
    最高.ok_or_else(|| "镜像源上没有可安装的测试版".to_string())
}

/// 下载指定版本的智语言开发套件安装包
/// 返回 (本地包路径, 校验 sha256 文本)
pub fn 下载工具包(版本号: &str, 目标目录: &Path) -> Result<(PathBuf, String), String> {
    // 支持 latest/最新 别名：先向镜像源解析出真实版本号
    let 版本号 = &解析版本别名(版本号)?;
    let 平台 = 平台标识();
    let 文件名 = format!("智语言开发套件{}-{}.zip", 版本号, 平台);
    let url = format!(
        "{}/{}/{}/{}",
        获取源地址(),
        url编码("下载"),
        url编码(版本号),
        url编码(&平台)
    );

    fs::create_dir_all(目标目录).map_err(|e| format!("创建目录失败: {}", e))?;
    let 包路径 = 目标目录.join(&文件名);

    let 字节 = 远程请求("GET", &url, None)?;
    fs::write(&包路径, &字节).map_err(|e| format!("写入安装包失败: {}", e))?;

    // 下载校验文件（/校验 路径段也须编码）
    let 校验url = format!("{}/{}", url, url编码("校验"));
    let 校验字节 = 远程请求("GET", &校验url, None)?;
    let 校验文本 = String::from_utf8_lossy(&校验字节).trim().to_string();
    // 校验文件首 token 为 sha256
    let sha文本 = 校验文本
        .split_whitespace()
        .next()
        .ok_or_else(|| "校验文件为空".to_string())?
        .to_string();

    Ok((包路径, sha文本))
}
