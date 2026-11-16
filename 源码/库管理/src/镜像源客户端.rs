use std::collections::HashMap;

use serde_json::Value;

/// 远程库信息
#[derive(Debug, Clone)]
pub struct 库信息 {
    pub 版本: Vec<String>,
    pub 描述: String,
    pub 许可证: String,
    /// 各版本的弃用状态（版本号 → 是否弃用/替代库/弃用提示），仅收录有弃用信息的版本
    pub 弃用: Vec<版本弃用>,
}

/// 某个版本的弃用状态
#[derive(Debug, Clone)]
pub struct 版本弃用 {
    pub 版本号: String,
    pub 是否弃用: bool,
    pub 替代库: String,
    pub 弃用提示: String,
}

/// 响应体读取上限：2GB。ureq 默认仅 10MB，封装库包可能超出该值。
const 最大响应字节: u64 = 2 * 1024 * 1024 * 1024;

/// 网络行为参数（来自 全局配置 [网络] 段，环境变量可覆盖用于测试）
struct 网络参数 {
    超时秒: u64,
    重试次数: u32,
    代理: String,
    离线模式: bool,
}

/// 读取全局配置的网络段；配置不可用时退回内置默认值。
fn 读取网络参数() -> 网络参数 {
    网络参数从配置(&pm::全局配置::读取配置())
}

/// 从已读取的配置对象提取网络参数（避免同一请求重复读盘）。
fn 网络参数从配置(配置: &pm::全局配置::全局配置) -> 网络参数 {
    let mut 参数 = 网络参数 {
        超时秒: 配置.网络.超时 as u64,
        重试次数: 配置.网络.重试次数,
        代理: 配置.网络.代理.clone(),
        离线模式: 配置.网络.离线模式,
    };
    // 环境变量覆盖：集成测试与临时排障用，优先级高于配置文件
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
        参数.超时秒 = 60;
    }
    参数
}

/// 库管理镜像源地址。
///
/// 优先级：环境变量 智语言_库镜像源_URL（测试/排障）> 全局配置 [镜像源].库镜像源
/// > 官方源常量。空配置值视为未设置，回退官方源。
pub fn 镜像源地址() -> String {
    if let Ok(环境变量) = std::env::var("智语言_库镜像源_URL") {
        if !环境变量.trim().is_empty() {
            return 环境变量.trim().trim_end_matches('/').to_string();
        }
    }
    // 库镜像源地址() 已内置「空 = 官方源」回退
    pm::全局配置::读取配置()
        .库镜像源地址()
        .trim_end_matches('/')
        .to_string()
}

/// 去除末尾斜杠，避免路径拼接重复；并把中文域名（IDN）主机名转为 Punycode。
///
/// 官方源是中文域名 https://库.智语言.org，HTTP 客户端按 RFC 3986 只接受 ASCII 主机名，
/// 直接发送会报 "invalid uri character"。此处统一做 IDNA ToASCII，
/// 使配置与提示中保留中文原文的同时，网络层可正常请求。
fn 获取源地址() -> String {
    let mut 地址 = 镜像源地址();
    while 地址.ends_with('/') {
        地址.pop();
    }
    pm::网址规范化::规范化网址(&地址)
}

/// 按 RFC 3986 对查询/路径片段做百分号编码（中文等非保留字符逐字节编码）
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

/// 拼接请求地址：对每个中文路径段做百分号编码。
///
/// 关键约束：ureq 不会自动对路径中的非 ASCII 字符做百分号编码，会原样发出
/// UTF-8 字节，导致服务端（Express）收到乱码路径而 404。因此所有路径段
/// （含固定段 "库"/"用户"/"登录" 与动态段 库名/版本）都必须在此统一编码。
fn 拼接地址(路径段: &[&str]) -> String {
    let mut 地址 = 获取源地址();
    for 段 in 路径段 {
        地址.push('/');
        地址.push_str(&url编码(段));
    }
    地址
}

/// 构造 HTTP 代理对象：超时取自全局配置 [网络].超时；
/// [网络].代理 为空时显式禁用代理（否则 ureq 会读 HTTP_PROXY 等环境变量，
/// 把 127.0.0.1 也送去代理，导致请求挂起或失败），非空时按配置走上游代理。
fn 新建代理(参数: &网络参数) -> ureq::Agent {
    let 连接超时 = std::time::Duration::from_secs(参数.超时秒.min(10).max(1));
    let 总超时 = std::time::Duration::from_secs(参数.超时秒);
    let 构建器 = ureq::Agent::config_builder()
        .timeout_connect(Some(连接超时))
        .timeout_global(Some(总超时))
        // 关闭"非 2xx 即错误"：由本模块统一读取响应体里的 {消息} 字段，
        // 才能把服务端的中文错误原文回传给用户，而不是干巴巴的 http status: 400
        .http_status_as_error(false);
    let 构建器 = match ureq::Proxy::new(参数.代理.trim()) {
        Ok(代理配置) if !参数.代理.trim().is_empty() => 构建器.proxy(Some(代理配置)),
        _ => 构建器.proxy(None),
    };
    构建器.build().new_agent()
}

/// 统一的远程请求：支持 GET/POST/PUT/DELETE，自动携带 Authorization。
///
/// 统一网络策略（规范 6.1.9 [网络] 段驱动）：
/// 1. 离线模式 → 立即短路报错，不发起任何连接；
/// 2. 超时/代理 → 取自全局配置；
/// 3. 重试次数 → 仅对连接层失败（网络抖动）重试，HTTP 4xx/5xx 业务错误不重试，
///    避免发布/弃用这类非幂等写操作被重复提交。
fn 远程请求(
    方法: &str,
    网址: &str,
    可选体: Option<Vec<u8>>,
    授权: Option<String>,
) -> Result<Vec<u8>, String> {
    let 参数 = 读取网络参数();
    if 参数.离线模式 {
        return Err(format!(
            "当前为离线模式，已跳过远程请求：{}。如需联网请执行「智语言 设置 离线模式 假」",
            网址
        ));
    }
    let 最大尝试 = 参数.重试次数.max(1);
    let mut 最后错误 = String::new();
    for 第几次 in 1..=最大尝试 {
        match 单次远程请求(&参数, 方法, 网址, 可选体.clone(), 授权.clone()) {
            Ok(数据) => return Ok(数据),
            Err((可重试, 错误)) => {
                最后错误 = 错误;
                if !可重试 || 第几次 == 最大尝试 {
                    break;
                }
                // 线性退避：第 n 次失败后等待 n×200ms，避开瞬时抖动
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
    网址: &str,
    可选体: Option<Vec<u8>>,
    授权: Option<String>,
) -> Result<Vec<u8>, (bool, String)> {
    let 代理 = 新建代理(参数);
    let 响应 = match 方法 {
        "GET" => {
            let b = 代理.get(网址);
            let b = if let Some(t) = 授权.as_deref() {
                b.header("Authorization", t)
            } else {
                b
            };
            b.call()
        }
        "POST" => {
            let b = 代理.post(网址);
            let b = if let Some(t) = 授权.as_deref() {
                b.header("Authorization", t)
            } else {
                b
            };
            let b = b.header("Content-Type", "application/json");
            b.send(可选体.unwrap_or_default())
        }
        "PUT" => {
            let b = 代理.put(网址);
            let b = if let Some(t) = 授权.as_deref() {
                b.header("Authorization", t)
            } else {
                b
            };
            // 发布走裸二进制流，显式声明内容类型，服务端 express.raw 才会填充 Buffer
            let b = b.header("Content-Type", "application/octet-stream");
            b.send(可选体.unwrap_or_default())
        }
        "DELETE" => {
            let b = 代理.delete(网址);
            let b = if let Some(t) = 授权.as_deref() {
                b.header("Authorization", t)
            } else {
                b
            };
            b.call()
        }
        _ => return Err((false, format!("不支持的 HTTP 方法: {}", 方法))),
    };
    match 响应 {
        Ok(mut 响应) => {
            let 状态 = 响应.status().as_u16();
            // 必须显式放开 ureq 默认 10MB 响应体上限：.库 封装包可能远超该值，
            // 否则下载大库会直接报 "larger than request limit"。
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
                // 业务错误（含 4xx/5xx）一律不重试：写操作非幂等，重复提交会造成脏数据
                if let Ok(值) = serde_json::from_slice::<Value>(&缓冲) {
                    if let Some(消息) = 值.get("消息").and_then(|v| v.as_str()) {
                        return Err((false, 消息.to_string()));
                    }
                }
                return Err((
                    false,
                    format!("服务返回错误 ({}): {}", 状态, String::from_utf8_lossy(&缓冲)),
                ));
            }
            Ok(缓冲)
        }
        // 连接层失败（拒绝连接/超时/DNS）判定为可重试
        Err(e) => Err((true, format!("请求失败: {}", e))),
    }
}

/// 登录结果（供 `账号 登录` 落盘凭据用）
pub struct 登录结果 {
    pub 令牌: String,
    pub 过期时间: i64,
}

/// 登录并获取会话令牌（对外暴露，供 `智语言 账号 登录` 使用）
pub fn 登录(用户名: &str, 密码: &str) -> Result<登录结果, String> {
    let url = 拼接地址(&["登录"]);
    let 体 = serde_json::json!({ "用户名": 用户名, "密码": 密码 })
        .to_string()
        .into_bytes();
    let 字节 = 远程请求("POST", &url, Some(体), None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析登录响应失败: {}", e))?;
    if let Some(令牌) = 值.get("令牌").and_then(|v| v.as_str()) {
        let 过期时间 = 值.get("过期时间").and_then(|v| v.as_i64()).unwrap_or(0);
        Ok(登录结果 { 令牌: 令牌.to_string(), 过期时间 })
    } else {
        Err("登录响应缺少令牌".to_string())
    }
}

/// 注册新用户（对外暴露，供 `智语言 账号 注册` 使用）；**会自动完成激活**
pub fn 注册(用户名: &str, 密码: &str, 邮箱: &str) -> Result<(), String> {
    let url = 拼接地址(&["用户"]);
    let 体 = serde_json::json!({ "用户名": 用户名, "密码": 密码, "邮箱": 邮箱 })
        .to_string()
        .into_bytes();
    let 字节 = 远程请求("POST", &url, Some(体), None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析注册响应失败: {}", e))?;
    let 激活令牌 = 值.get("激活令牌").and_then(|v| v.as_str()).ok_or_else(|| {
        format!(
            "注册响应缺少激活令牌，服务端响应：{}",
            String::from_utf8_lossy(&字节)
        )
    })?;
    激活(激活令牌)
}

/// 激活账号（注册后需激活才能发布）
fn 激活(激活令牌: &str) -> Result<(), String> {
    // 查询参数名同样是中文，必须编码：ureq 不会自动编码非 ASCII 字符，
    // 未编码时服务端读不到 请求.query.令牌，会返回"缺少激活令牌"
    let url = format!(
        "{}?{}={}",
        拼接地址(&["激活"]),
        url编码("令牌"),
        url编码(激活令牌)
    );
    let 字节 = 远程请求("GET", &url, None, None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析激活响应失败: {}", e))?;
    if 值.get("结果").and_then(|v| v.as_str()) == Some("激活成功") {
        Ok(())
    } else {
        Err("激活失败".to_string())
    }
}

/// 获取发布用的会话令牌。**令牌来源优先级**（从高到低）：
///   ① 环境变量 `智语言_库令牌`（直接指定令牌，CI/自动化最可控）
///   ② 环境变量 `智语言_库用户名` + `智语言_库密码`（+ **真实邮箱** `智语言_库邮箱`，仅首次注册需要）
///      —— **不落盘**：避免把 CI 的测试凭据写进用户真实的 凭据.配置 ✗
///   ③ 本地凭据 `<全局目录>/配置/凭据.配置`（`账号 登录` 写入；过期自动忽略，设计文档 §4.2）
///   ④ **没有任何凭据 ⇒ 明确拒绝**（绝不使用内置默认账号）
///
/// ⚠ 2026-11-16 安全修复（高危：默认发布账号导致库可被劫持）：
///   此前 ④ 用**硬编码在可执行文件里**的账号（用户名/明文密码/`{用户名}@example.com` 邮箱）
///   自动注册并登录 ⇒ 所有未配置账号的用户发布的库都归到同一个**公开可读出密码**的账号下，
///   任何人可登录后对这些库发新版本（植入恶意代码）或标为弃用。
///   同时 ② 分支曾把邮箱默认成 `{用户名}@example.com`（example.com 是保留测试域名，
///   收不到验证邮件、也无法用于找回账号）⇒ **注册必须有真实邮箱**，缺邮箱时不再自动注册。
///
///   服务端侧配套动作（不在本仓库）：让历史默认账号失效、审计其名下库、
///   注册强制邮箱验证 —— 客户端已不再产生该账号的新会话，但历史数据需服务端处理。
pub fn 获取发布令牌() -> Result<String, String> {
    if let Ok(令牌) = std::env::var("智语言_库令牌") {
        if !令牌.trim().is_empty() {
            return Ok(令牌);
        }
    }
    // ② 环境变量账号密码（邮箱**必须显式提供**，不再默认 example.com）
    if let (Ok(用户名), Ok(密码)) = (std::env::var("智语言_库用户名"), std::env::var("智语言_库密码")) {
        if !用户名.trim().is_empty() {
            let 邮箱 = std::env::var("智语言_库邮箱")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            return match 登录(&用户名, &密码) {
                Ok(结果) => Ok(结果.令牌),
                // 登录失败通常是账号不存在：**只有提供了真实邮箱才允许自举注册**
                Err(登录错误) => {
                    let 邮箱 = 邮箱.ok_or_else(|| {
                        format!(
                            "登录失败（{}），且未提供 智语言_库邮箱，拒绝自动注册。\n\
                             注册需要一个**真实可收信**的邮箱（用于验证与找回账号）。\n\
                             请改用以下任一方式：\n\
                              · 设置 智语言_库邮箱 后重试（首次发布会自动注册）\n\
                              · 先执行 `智语言 账号 注册 <用户名> <密码> <邮箱>`，再 `智语言 账号 登录 <用户名> <密码>`\n\
                              · 或用 智语言_库令牌 直接提供会话令牌",
                            登录错误
                        )
                    })?;
                    注册(&用户名, &密码, &邮箱).map_err(|e| format!("自动注册失败: {}", e))?;
                    登录(&用户名, &密码)
                        .map(|r| r.令牌)
                        .map_err(|e| format!("注册后登录失败: {}", e))
                }
            };
        }
    }
    // ③ 本地凭据（过期视为无）
    if let Some(凭据) = super::凭据::读取() {
        return Ok(凭据.令牌);
    }
    // ④ 无凭据 ⇒ 拒绝（**不再使用内置默认账号**）
    Err("未找到库镜像源的发布凭据，已拒绝发布。\n\
         为避免「所有用户共用一个可被公开读出密码的账号」，客户端不再内置默认发布账号。\n\
         请任选一种方式先完成身份配置：\n\
         · 智语言 账号 登录 <用户名> <密码>      （凭据写入 <全局目录>/配置/凭据.配置）\n\
         · 智语言 账号 注册 <用户名> <密码> <邮箱>  （需真实邮箱）\n\
         · 环境变量 智语言_库令牌=<会话令牌>      （CI/自动化推荐）\n\
         · 环境变量 智语言_库用户名 + 智语言_库密码（+ 智语言_库邮箱，仅首次自动注册需要）"
        .to_string())
}

/// 搜索结果项（输出格式符合规范 §6.3.4：库名 版本 许可证 描述）
#[derive(Debug, Clone)]
pub struct 搜索项 {
    pub 名称: String,
    pub 版本: String,
    pub 许可证: String,
    pub 描述: String,
}

/// 搜索远程库，返回结构化结果（名称/版本/许可证/描述）
/// 搜索镜像源库。
///
/// ⚠ 2026-10-03 修复：原先签名是 `-> Vec<搜索项>` 且内部 `远程搜索(..).unwrap_or_default()`，
///   把**网络失败 / 5xx / 响应解析失败**全部静默吞成**空列表** ⇒ `库 搜索` 一律打印
///   「未找到匹配的库」，用户无法区分"镜像源不可达"与"确实没有该库"（实测：把镜像源指到
///   错误端口时仍报「未找到匹配的库」✗）。⇒ 改为返回 `Result`，由调用方如实报错。
pub fn 搜索(关键词: &str) -> Result<Vec<搜索项>, String> {
    远程搜索(关键词)
}

fn 远程搜索(关键词: &str) -> Result<Vec<搜索项>, String> {
    let url = format!(
        "{}?{}={}",
        拼接地址(&["库"]),
        url编码("关键词"),
        url编码(关键词)
    );
    let 字节 = 远程请求("GET", &url, None, None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析搜索响应失败: {}", e))?;
    let mut 结果 = Vec::new();
    if let Some(列表) = 值.get("库列表").and_then(|v| v.as_array()) {
        for 项 in 列表 {
            // 库名缺失则跳过该项
            let 名称 = match 项.get("库名").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // 版本：优先单行"版本"，否则取版本列表首项（兼容两种服务端形态）
            let 版本 = 项.get("版本").and_then(|v| v.as_str()).map(|s| s.to_string())
                .or_else(|| 项.get("版本列表").and_then(|v| v.as_array())
                    .and_then(|a| a.first()).and_then(|x| x.get("版本号")).and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or("".to_string());
            let 许可证 = 项.get("许可证").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let 描述 = 项.get("描述").and_then(|v| v.as_str()).unwrap_or("").to_string();
            结果.push(搜索项 { 名称, 版本, 许可证, 描述 });
        }
    }
    Ok(结果)
}

/// 查询远程库的可用版本号列表
pub fn 查询版本(库名: &str) -> Result<Vec<String>, String> {
    远程查询版本(库名)
}

fn 远程查询版本(库名: &str) -> Result<Vec<String>, String> {
    let url = 拼接地址(&["库", 库名]);
    let 字节 = 远程请求("GET", &url, None, None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析库信息响应失败: {}", e))?;
    let mut 版本 = Vec::new();
    if let Some(列表) = 值.get("版本列表").and_then(|v| v.as_array()) {
        for 项 in 列表 {
            if let Some(v) = 项.get("版本号").and_then(|x| x.as_str()) {
                版本.push(v.to_string());
            }
        }
    }
    Ok(版本)
}

/// 下载远程库，返回 库名 -> 字节 的映射
pub fn 下载库(库名: &str, 版本: &str) -> Result<HashMap<String, Vec<u8>>, String> {
    远程下载库(库名, 版本)
}

fn 远程下载库(库名: &str, 版本: &str) -> Result<HashMap<String, Vec<u8>>, String> {
    let url = 拼接地址(&["库", 库名, 版本]);
    let 字节 = 远程请求("GET", &url, None, None)?;
    let mut 结果 = HashMap::new();
    结果.insert(库名.to_string(), 字节);
    Ok(结果)
}

/// 查询远程库的元数据（版本 / 描述 / 许可证）
pub fn 查询库信息(库名: &str) -> Result<库信息, String> {
    let url = 拼接地址(&["库", 库名]);
    let 字节 = 远程请求("GET", &url, None, None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析库信息响应失败: {}", e))?;
    let mut 版本 = Vec::new();
    let mut 弃用 = Vec::new();
    if let Some(列表) = 值.get("版本列表").and_then(|v| v.as_array()) {
        for 项 in 列表 {
            if let Some(v) = 项.get("版本号").and_then(|x| x.as_str()) {
                版本.push(v.to_string());
            }
            // 弃用状态：兼容 boolean 与 0/1 数字两种服务端形态
            let 版本号 = 项.get("版本号").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let 是否弃用 = match 项.get("是否弃用") {
                Some(Value::Bool(b)) => *b,
                Some(Value::Number(n)) => n.as_i64().unwrap_or(0) == 1,
                _ => false,
            };
            let 替代库 = 项.get("替代库").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let 弃用提示 = 项.get("弃用提示").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if 是否弃用 || !替代库.is_empty() || !弃用提示.is_empty() {
                弃用.push(版本弃用 {
                    版本号,
                    是否弃用,
                    替代库,
                    弃用提示,
                });
            }
        }
    }
    let 描述 = 值
        .get("描述")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let 许可证 = 值
        .get("许可证")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(库信息 {
        版本,
        描述,
        许可证,
        弃用,
    })
}

/// 发布库到远程镜像源（裸二进制 PUT + 会话令牌）
pub fn 发布(库名: &str, 版本: &str, 库数据: &[u8]) -> Result<(), String> {
    let 令牌 = 获取发布令牌()?;
    let url = 拼接地址(&["库", 库名, 版本]);
    let 授权 = format!("Bearer {}", 令牌);
    远程请求("PUT", &url, Some(库数据.to_vec()), Some(授权))?;
    Ok(())
}

/// 某个版本的依赖声明
#[derive(Debug, Clone)]
pub struct 版本依赖 {
    pub 版本号: String,
    pub 运行依赖: Vec<String>,
    pub 开发依赖: Vec<String>,
    pub 构建依赖: Vec<String>,
}

/// 从依赖声明 JSON 对象里取出某一类依赖的库名列表
fn 取依赖分类(依赖: Option<&Value>, 分类: &str) -> Vec<String> {
    依赖
        .and_then(|v| v.get(分类))
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// 查询远程库各版本的依赖声明（GET /库/{库名}/依赖）
pub fn 查询依赖(库名: &str) -> Result<Vec<版本依赖>, String> {
    let url = 拼接地址(&["库", 库名, "依赖"]);
    let 字节 = 远程请求("GET", &url, None, None)?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析依赖响应失败: {}", e))?;
    let mut 结果 = Vec::new();
    if let Some(列表) = 值.get("版本列表").and_then(|v| v.as_array()) {
        for 项 in 列表 {
            let 版本号 = 项
                .get("版本号")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let 依赖 = 项.get("依赖");
            结果.push(版本依赖 {
                版本号,
                运行依赖: 取依赖分类(依赖, "运行依赖"),
                开发依赖: 取依赖分类(依赖, "开发依赖"),
                构建依赖: 取依赖分类(依赖, "构建依赖"),
            });
        }
    }
    Ok(结果)
}

//// 查询某版本库的依赖声明（运行依赖/开发依赖/构建依赖）。
/// 从 查询依赖 的全版本结果里按 版本号 精确匹配；版本号匹配不上时返回空声明（不报错，
/// 由上层决定是否忽略，避免"镜像源无依赖信息"导致主库安装整体失败）。
pub fn 查询版本依赖(库名: &str, 版本号: &str) -> Result<Vec<String>, String> {
    let 列表 = 查询依赖(库名)?;
    for 项 in 列表 {
        if 项.版本号 == 版本号 {
            return Ok(项.运行依赖);
        }
    }
    Ok(Vec::new())
}

// 弃用某个已发布版本（POST /库/{库名}/{版本}/弃用，需作者身份）
pub fn 弃用版本(库名: &str, 版本: &str, 替代库: &str, 提示信息: &str) -> Result<(), String> {
    let 令牌 = 获取发布令牌()?;
    let url = 拼接地址(&["库", 库名, 版本, "弃用"]);
    let 体 = serde_json::json!({ "替代库": 替代库, "提示信息": 提示信息 })
        .to_string()
        .into_bytes();
    let 授权 = format!("Bearer {}", 令牌);
    let 字节 = 远程请求("POST", &url, Some(体), Some(授权))?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析弃用响应失败: {}", e))?;
    if 值.get("结果").and_then(|v| v.as_str()) == Some("已标记弃用") {
        Ok(())
    } else {
        Err(format!("弃用失败：{}", String::from_utf8_lossy(&字节)))
    }
}

/// 校验会话令牌是否有效（POST /验证令牌），返回 (是否有效, 用户名)
pub fn 验证令牌(令牌: &str) -> Result<(bool, String), String> {
    let url = 拼接地址(&["验证令牌"]);
    let 体 = serde_json::json!({ "令牌": 令牌 }).to_string().into_bytes();
    let 字节 = 远程请求("POST", &url, Some(体), None)?;
    let 值: Value =
        serde_json::from_slice(&字节).map_err(|e| format!("解析验证令牌响应失败: {}", e))?;
    let 有效 = 值.get("有效").and_then(|v| v.as_bool()).unwrap_or(false);
    let 用户名 = 值
        .get("用户名")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((有效, 用户名))
}

/// 登出并注销会话令牌（DELETE /令牌）
pub fn 登出(令牌: &str) -> Result<(), String> {
    let url = 拼接地址(&["令牌"]);
    let 授权 = format!("Bearer {}", 令牌);
    let 字节 = 远程请求("DELETE", &url, None, Some(授权))?;
    let 值: Value = serde_json::from_slice(&字节).map_err(|e| format!("解析登出响应失败: {}", e))?;
    if 值.get("结果").and_then(|v| v.as_str()) == Some("已登出") {
        Ok(())
    } else {
        Err(format!("登出失败：{}", String::from_utf8_lossy(&字节)))
    }
}

/// 获取会话令牌（对外暴露，供 验证令牌/登出 等需要显式令牌的场景使用）
pub fn 获取会话令牌() -> Result<String, String> {
    获取发布令牌()
}

/// 登出并**清除本地凭据**：令牌来源同 `获取发布令牌` 的优先级（env 令牌 > 凭据文件）。
/// 成功后会删除 `<全局目录>/配置/凭据.配置`（设计文档 §4.4「登出同时删除本地凭据」）。
pub fn 登出当前() -> Result<String, String> {
    if let Ok(令牌) = std::env::var("智语言_库令牌") {
        if !令牌.trim().is_empty() {
            登出(&令牌)?;
            return Ok("令牌（来自环境变量 智语言_库令牌）".to_string());
        }
    }
    match super::凭据::读取() {
        Some(凭据) => {
            登出(&凭据.令牌)?;
            super::凭据::删除();
            Ok(凭据.用户名)
        }
        None => Err("本地没有已保存的登录凭据，请先执行「智语言 账号 登录」（或设置环境变量 智语言_库令牌）".to_string()),
    }
}

/// 当前凭据状态，供 `账号 状态` 展示：返回 (用户名, 过期时间, 是否仍在有效期内)
pub fn 当前凭据状态() -> Option<(String, i64, bool)> {
    let 凭据 = super::凭据::读取原始()?;
    let 有效 = super::凭据::读取().is_some();
    Some((凭据.用户名, 凭据.过期时间, 有效))
}
