// 智语言测试用镜像源 (模拟服务器)
// 支持：上传(PUT, 需 Basic Auth) / 搜索 / 信息 / 下载(GET, 免认证)
// 上传默认账户密码：admin / admin123
use std::collections::HashMap;
use std::fs::{create_dir_all, read, write};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

// ── 基础64 编码（仅 std，与 镜像源客户端 保持一致） ──
fn base64_编码(输入: &str) -> String {
    const 表: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let 字节 = 输入.as_bytes();
    let 长度 = 字节.len();
    let mut 结果 = String::new();
    let mut i = 0;
    while i < 长度 {
        let 片段 = [
            字节[i],
            if i + 1 < 长度 { 字节[i + 1] } else { 0 },
            if i + 2 < 长度 { 字节[i + 2] } else { 0 },
        ];
        let 值 = (片段[0] as u32) << 16 | (片段[1] as u32) << 8 | 片段[2] as u32;
        let 索引组 = [
            ((值 >> 18) & 0x3F) as usize,
            ((值 >> 12) & 0x3F) as usize,
            ((值 >> 6) & 0x3F) as usize,
            (值 & 0x3F) as usize,
        ];
        let 剩余 = 长度 - i;
        let 本组字节 = 剩余.min(3);
        let 实字符数 = 本组字节 + 1; // 1输入→2字符, 2输入→3字符, 3输入→4字符
        for j in 0..4 {
            if j < 实字符数 {
                结果.push(表[索引组[j]] as char);
            } else {
                结果.push('=');
            }
        }
        i += 3;
    }
    结果
}

// ── 请求头提取 ──
fn 提取请求头(全部头: &str, 名称: &str) -> String {
    for 行 in 全部头.split("\r\n") {
        let 部分: Vec<&str> = 行.splitn(2, ':').collect();
        if 部分.len() == 2 && 部分[0].trim().eq_ignore_ascii_case(名称) {
            return 部分[1].trim().to_string();
        }
    }
    String::new()
}

// ── 内容长度提取 ──
fn 提取内容长度(头文本: &str) -> usize {
    let 原始 = 提取请求头(头文本, "Content-Length");
    if 原始.is_empty() {
        return 0;
    }
    原始.parse::<usize>().unwrap_or(0)
}

// ── 查询参数提取 ──
fn 提取查询参数(查询: &str, 名称: &str) -> String {
    for 段 in 查询.split('&') {
        let 键值: Vec<&str> = 段.splitn(2, '=').collect();
        if 键值.len() == 2 && 键值[0] == 名称 {
            return 键值[1].to_string();
        }
    }
    String::new()
}

// ── URL 解码（支持 %XX） ──
fn 网址解码(输入: &str) -> String {
    let 字节 = 输入.as_bytes();
    let 长度 = 字节.len();
    let mut 结果: Vec<u8> = Vec::with_capacity(长度);
    let mut i = 0;
    while i < 长度 {
        if 字节[i] == b'%' && i + 2 < 长度 {
            let 高 = (字节[i + 1] as char).to_digit(16);
            let 低 = (字节[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (高, 低) {
                结果.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        if 字节[i] == b'+' {
            结果.push(b' ');
        } else {
            结果.push(字节[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&结果).trim().to_string()
}

// ── 上传授权校验（默认账户 admin / admin123） ──
fn 验证上传授权(请求头: &str) -> bool {
    请求头.contains("Authorization: Basic ")
        && 请求头.contains(&base64_编码("admin:admin123"))
}

// ── 存储根目录 ──
fn 库存储根() -> PathBuf {
    let 根 = std::env::temp_dir().join("智语言_mock库存储");
    let _ = create_dir_all(&根);
    根
}

// ── 索引文件 ──
fn 索引路径() -> PathBuf {
    库存储根().join("索引.txt")
}

fn 加载索引() -> HashMap<String, Vec<String>> {
    let mut 映射 = HashMap::new();
    if let Ok(内容) = read(索引路径()) {
        let 文本 = String::from_utf8_lossy(&内容);
        for 行 in 文本.lines() {
            let 部分: Vec<&str> = 行.split_whitespace().collect();
            if 部分.len() >= 2 {
                映射.entry(部分[0].to_string())
                    .or_insert_with(Vec::new)
                    .push(部分[1].to_string());
            }
        }
    }
    映射
}

fn 保存索引(映射: &HashMap<String, Vec<String>>) {
    let mut 文本 = String::new();
    let mut 键: Vec<&String> = 映射.keys().collect();
    键.sort();
    for 名 in 键 {
        if let Some(版本列表) = 映射.get(名) {
            for 版本 in 版本列表 {
                文本.push_str(&format!("{} {}\n", 名, 版本));
            }
        }
    }
    let _ = write(索引路径(), 文本.as_bytes());
}

fn 更新发布索引(库名: &str, 版本: &str) {
    let mut 映射 = 加载索引();
    let 列表 = 映射.entry(库名.to_string()).or_insert_with(Vec::new);
    if !列表.contains(&版本.to_string()) {
        列表.push(版本.to_string());
        列表.sort();
    }
    保存索引(&映射);
}

// ── 读取已发布库二进制 ──
fn 读取发布库(库名: &str, 版本: &str) -> Option<Vec<u8>> {
    let 路径 = 库存储根().join(库名).join(format!("{}{}", 库名, 版本));
    read(&路径).ok()
}

// ── 已发布版本 ──
fn 已发布版本(库名: &str) -> Vec<String> {
    加载索引().get(库名).cloned().unwrap_or_default()
}

// ── 全部已发布库名（含内置示例，便于搜索非空） ──
fn 全部已发布() -> Vec<String> {
    let mut 列表: Vec<String> = 加载索引().keys().cloned().collect();
    列表.push("示例网络库".to_string());
    列表.sort();
    列表.dedup();
    列表
}

// ── HTTP 响应结构（二进制安全） ──
struct HttpResponse {
    状态: u16,
    状态文本: String,
    头: Vec<(String, String)>,
    正文: Vec<u8>,
}

impl HttpResponse {
    fn new(状态: u16, 状态文本: &str) -> Self {
        HttpResponse {
            状态,
            状态文本: 状态文本.to_string(),
            头: vec![("Connection".to_string(), "close".to_string())],
            正文: Vec::new(),
        }
    }
}

fn 文本响应(状态: u16, 状态文本: &str, 正文: Vec<u8>) -> HttpResponse {
    let mut 响应 = HttpResponse::new(状态, 状态文本);
    响应
        .头
        .push(("Content-Type".to_string(), "application/json".to_string()));
    响应
        .头
        .push(("Content-Length".to_string(), 正文.len().to_string()));
    响应.正文 = 正文;
    响应
}

fn 错误响应(标题: &str, 消息: &str, 代码: u16) -> HttpResponse {
    let 正文 = format!(
        "{{\"标题\":\"{}\",\"消息\":\"{}\",\"代码\":{}}}",
        标题, 消息, 代码
    )
    .into_bytes();
    文本响应(代码, 标题, 正文)
}

fn 默认响应() -> HttpResponse {
    错误响应("不支持的请求", "路径或方法不被支持", 404)
}

fn 发送响应(流: &mut TcpStream, 响应: &HttpResponse) {
    let mut 输出: Vec<u8> = Vec::new();
    输出.extend_from_slice(
        format!("HTTP/1.1 {} {}\r\n", 响应.状态, 响应.状态文本).as_bytes(),
    );
    for (名, 值) in &响应.头 {
        输出.extend_from_slice(format!("{}: {}\r\n", 名, 值).as_bytes());
    }
    输出.extend_from_slice(b"\r\n");
    输出.extend_from_slice(&响应.正文);
    let _ = 流.write_all(&输出);
    let _ = 流.flush();
}

// ── 读取完整请求（按 Content-Length 读取二进制体，避免截断） ──
fn 查找头体分界(全部: &[u8]) -> Option<usize> {
    if 全部.len() < 4 {
        return None;
    }
    for i in 0..全部.len() - 3 {
        if &全部[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

fn 读取完整请求(流: &mut TcpStream) -> Vec<u8> {
    let _ = 流.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let mut 全部: Vec<u8> = Vec::new();
    let mut 缓冲区 = [0u8; 65536];
    let 最大 = 64 * 1024 * 1024;
    loop {
        match 流.read(&mut 缓冲区) {
            Ok(0) => break,
            Ok(n) => {
                全部.extend_from_slice(&缓冲区[..n]);
                if let Some(分界) = 查找头体分界(&全部) {
                    let 头文本 =
                        String::from_utf8_lossy(&全部[..分界]).to_string();
                    let 需要 = 提取内容长度(&头文本);
                    let 已读体 = 全部.len() - (分界 + 4);
                    if 已读体 >= 需要 {
                        break;
                    }
                }
                if 全部.len() > 最大 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    全部
}

// ── 解析请求行（方法 + 路径） ──
fn 解析首行(全部: &[u8]) -> (String, String) {
    let 行尾 = 全部
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(全部.len());
    let 行 = String::from_utf8_lossy(&全部[..行尾]).trim().to_string();
    let mut 部分 = 行.split_whitespace();
    let 方法 = 部分.next().unwrap_or("").to_string();
    let 路径 = 部分.next().unwrap_or("").to_string();
    (方法, 路径)
}

// ── 响应处理 ──
fn 库搜索响应(路径: &str) -> HttpResponse {
    let 查询 = 路径.split('?').nth(1).unwrap_or("");
    let 关键词 = 网址解码(&提取查询参数(查询, "关键词"));
    let 全部 = 全部已发布();
    let 命中: Vec<String> = 全部
        .into_iter()
        .filter(|库名| 关键词.is_empty() || 库名.contains(&关键词))
        .collect();
    let 主体 = 命中
        .iter()
        .map(|名| format!("\"{}\"", 名))
        .collect::<Vec<_>>()
        .join(",");
    let 正文 = format!("[{}]", 主体).into_bytes();
    文本响应(200, "OK", 正文)
}

fn 库信息响应(库名: &str) -> HttpResponse {
    let 库名 = 网址解码(库名);
    let mut 版本 = 已发布版本(&库名);
    // 示例网络库在搜索结果中展示，但无真实发布索引，给一个默认版本以便信息接口可用
    if 版本.is_empty() && 库名 == "示例网络库" {
        版本 = vec!["1.0.0".to_string()];
    }
    if 版本.is_empty() {
        return 错误响应("未找到", "库不存在", 404);
    }
    let 版本串 = 版本
        .iter()
        .map(|v| format!("\"{}\"", v))
        .collect::<Vec<_>>()
        .join(",");
    // G9：信息接口补充 描述 / 许可证 字段
    let 描述 = if 库名 == "示例网络库" {
        "智语言网络示例库".to_string()
    } else {
        format!("智语言库 {}", 库名)
    };
    let 许可证 = "MIT".to_string();
    let 正文 = format!(
        "{{\"库名\":\"{}\",\"版本\":[{}],\"描述\":\"{}\",\"许可证\":\"{}\"}}",
        库名, 版本串, 描述, 许可证
    )
    .into_bytes();
    文本响应(200, "OK", 正文)
}

fn 库下载响应(库名: &str, 版本: &str) -> HttpResponse {
    let 库名 = 网址解码(库名);
    let 版本 = 网址解码(版本);
    match 读取发布库(&库名, &版本) {
        Some(数据) => {
            let mut 响应 = HttpResponse::new(200, "OK");
            响应.头.push((
                "Content-Type".to_string(),
                "application/octet-stream".to_string(),
            ));
            响应.头.push((
                "Content-Length".to_string(),
                数据.len().to_string(),
            ));
            响应.正文 = 数据;
            响应
        }
        None => 错误响应("未找到", "库版本不存在", 404),
    }
}

fn 库发布响应(正文: &[u8], 路径: &str, 头文本: &str) -> HttpResponse {
    let 段: Vec<&str> = 路径
        .trim_start_matches("/api/v1/库/")
        .split('/')
        .collect();
    if 段.len() < 2 {
        return 默认响应();
    }
    let 库名 = 段[0];
    let 版本 = 段[1];
    if !验证上传授权(头文本) {
        return 错误响应("未授权", "需要上传凭证 (admin/admin123)", 401);
    }
    let 存储根 = 库存储根();
    let 库目录 = 存储根.join(库名);
    let _ = create_dir_all(&库目录);
    let 文件路径 = 库目录.join(format!("{}{}", 库名, 版本));
    match write(&文件路径, 正文) {
        Ok(_) => {
            更新发布索引(库名, 版本);
            文本响应(
                200,
                "OK",
                format!(
                    "{{\"标题\":\"已发布\",\"库名\":\"{}\",\"版本\":\"{}\"}}",
                    库名, 版本
                )
                .into_bytes(),
            )
        }
        Err(e) => 错误响应("写入失败", &e.to_string(), 500),
    }
}

fn 处理请求(流: &mut TcpStream) {
    let 原始 = 读取完整请求(流);
    if 原始.is_empty() {
        return;
    }
    let 分界 = 查找头体分界(&原始).unwrap_or(原始.len());
    let 头文本 = String::from_utf8_lossy(&原始[..分界]).to_string();
    let (方法, 路径) = 解析首行(&原始);
    let 正文 = 原始[分界 + 4..].to_vec();

    let 响应 = if 方法 == "PUT" && 路径.starts_with("/api/v1/库/") {
        库发布响应(&正文, &路径, &头文本)
    } else if 路径.starts_with("/api/v1/库?") {
        库搜索响应(&路径)
    } else if 路径.starts_with("/api/v1/库/") {
        let 段: Vec<&str> = 路径
            .trim_start_matches("/api/v1/库/")
            .split('/')
            .collect();
        if 段.len() >= 2 {
            库下载响应(段[0], 段[1])
        } else if 段.len() == 1 && !段[0].is_empty() {
            库信息响应(段[0])
        } else {
            默认响应()
        }
    } else {
        默认响应()
    };
    发送响应(流, &响应);
}

// ── 单元测试：上传(PUT 需认证) / 下载(GET 免认证) 逻辑 ──
// 沙箱封锁进程间 TCP（回环与外网卡均连接超时），故对确定性处理函数做进程内验证，
// 绕过被封的 socket 层，直接覆盖「上传需默认账户密码 / 下载免密」规范语义。
#[cfg(test)]
mod 测试 {
    use super::*;

    const 正确凭证: &str = "Authorization: Basic YWRtaW46YWRtaW4xMjM=";

    #[test]
    fn 上传授权_正确账户密码_通过() {
        assert!(验证上传授权(正确凭证));
        // 头包含额外字段也应通过
        assert!(验证上传授权("Host: x\r\nAuthorization: Basic YWRtaW46YWRtaW4xMjM=\r\n"));
    }

    #[test]
    fn 上传授权_错误密码_拒绝() {
        assert!(!验证上传授权("Authorization: Basic YWRtaW46d3Jvbmc="));
        assert!(!验证上传授权(""));
        assert!(!验证上传授权("Authorization: Bearer abc"));
    }

    #[test]
    fn 上传_带认证_发布成功() {
        let 库名 = "单元测试库_上传";
        let 版本 = "1.0.0";
        let 数据 = b"\x89PNG test-bytes\x00";
        let 响应 = 库发布响应(数据, &format!("/api/v1/库/{}/{}", 库名, 版本), 正确凭证);
        assert_eq!(响应.状态, 200, "带默认凭证上传应返回 200");
        // 已写入存储，且索引已登记
        assert!(读取发布库(库名, 版本).is_some());
        assert!(已发布版本(库名).contains(&版本.to_string()));
    }

    #[test]
    fn 上传_无认证_返回401() {
        let 库名 = "单元测试库_无认证";
        let 版本 = "1.0.0";
        let 数据 = b"should-not-be-stored";
        let 响应 = 库发布响应(数据, &format!("/api/v1/库/{}/{}", 库名, 版本), "");
        assert_eq!(响应.状态, 401, "无凭证上传应返回 401 未授权");
        assert!(读取发布库(库名, 版本).is_none(), "无凭证不应落盘");
    }

    #[test]
    fn 下载_免密_返回已发布内容() {
        let 库名 = "单元测试库_下载";
        let 版本 = "2.1.0";
        let 数据: Vec<u8> = vec![1, 2, 3, 4, 5, 255, 0, 128];
        let 发布 = 库发布响应(&数据, &format!("/api/v1/库/{}/{}", 库名, 版本), 正确凭证);
        assert_eq!(发布.状态, 200);
        // 下载免密，且返回字节与上传一致
        let 下载 = 库下载响应(库名, 版本);
        assert_eq!(下载.状态, 200, "下载应免密返回 200");
        assert_eq!(下载.正文, 数据, "下载内容应与上传一致");
    }

    #[test]
    fn 下载_不存在版本_返回404() {
        let 下载 = 库下载响应("单元测试库_不存在", "9.9.9");
        assert_eq!(下载.状态, 404);
    }
}

fn main() {
    let 监听器 = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[mock] 绑定失败: {}", e);
            return;
        }
    };
    let 端口 = 监听器.local_addr().unwrap().port();
    println!("端口: {}", 端口);
    let _ = std::io::stdout().flush();
    // 端口可靠落盘（Start-Process 重定向的 stdout 管道在进程退出前可能不刷新）
    let _ = std::fs::write(std::env::temp_dir().join("z_mock_port.txt"), 端口.to_string());
    for 连接 in 监听器.incoming() {
        match 连接 {
            Ok(mut 流) => 处理请求(&mut 流),
            Err(e) => eprintln!("[mock] 连接错误: {}", e),
        }
    }
}
