// 文件: 标准库功能/文本系统函数.rs
// 职责: 文本编解码辅助（Base64，与解释器侧一致）

pub(crate) fn Base64编码(数据: &str) -> String {
    const 编码表: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let 字节 = 数据.as_bytes();
    let mut 结果 = String::new();
    for 块 in 字节.chunks(3) {
        let 字节0 = 块[0] as u32;
        let 字节1 = if 块.len() > 1 { 块[1] as u32 } else { 0 };
        let 字节2 = if 块.len() > 2 { 块[2] as u32 } else { 0 };
        let 合并 = (字节0 << 16) | (字节1 << 8) | 字节2;
        结果.push(编码表[((合并 >> 18) & 63) as usize] as char);
        结果.push(编码表[((合并 >> 12) & 63) as usize] as char);
        结果.push(if 块.len() > 1 { 编码表[((合并 >> 6) & 63) as usize] } else { b'=' } as char);
        结果.push(if 块.len() > 2 { 编码表[(合并 & 63) as usize] } else { b'=' } as char);
    }
    结果
}

/// Base64 解码（标准字母表 + `=` 填充；忽略空白/换行，兼容多行 PEM 体）。
///
/// ★ 2026-10-19 修复（核心库全面测试发现）：原实现 `_ => continue` **静默跳过非法字符**、
///   末尾 `from_utf8_lossy` **静默替换非法字节**，导致 `工具.解码方案("BASE64", "!!!!")`
///   返回空串而不报错 ⇒ 与同模块「不支持方案必须抛异常」的口径自相矛盾（F14 同类），
///   调用方会把空串当成合法解码结果继续使用 ✗。
///   ⇒ 改为 `Result`：非法字符 / 长度非法（mod 4 == 1）/ 填充错位 / 解码字节非 UTF-8
///   全部返回明确错误，由调用方抛可捕获异常（本文件不直接抛，保持纯函数、可单测）。
pub(crate) fn Base64解码(数据: &str) -> Result<String, String> {
    // 1) 去掉空白（换行/制表/空格）后统一处理
    let 清理: Vec<char> = 数据.chars().filter(|c| !c.is_whitespace()).collect();
    // 2) 取出尾部填充
    let mut 主体长度 = 清理.len();
    let mut 填充数 = 0;
    while 主体长度 > 0 && 清理[主体长度 - 1] == '=' {
        主体长度 -= 1;
        填充数 += 1;
    }
    if 填充数 > 2 {
        return Err(format!("BASE64 填充符过多（{} 个 '='）", 填充数));
    }
    if 填充数 > 0 && (主体长度 + 填充数) % 4 != 0 {
        return Err("BASE64 填充位置非法（长度不是 4 的倍数）".to_string());
    }
    // 3) 字符合法性 + 长度合法性（mod 4 == 1 不可能由任何字节序列产生）
    for (序号, 字符) in 清理[..主体长度].iter().enumerate() {
        let 合法 = matches!(字符, 'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/');
        if !合法 {
            return Err(format!("BASE64 数据包含非法字符 '{}'（第 {} 个字符）", 字符, 序号 + 1));
        }
    }
    if 主体长度 % 4 == 1 {
        return Err(format!("BASE64 数据长度非法（{} 个有效字符）", 主体长度));
    }
    // 4) 解码
    let mut 结果 = Vec::new();
    let mut 缓冲 = 0u32;
    let mut 位数 = 0;
    for 字符 in &清理[..主体长度] {
        let 值 = match 字符 {
            'A'..='Z' => *字符 as u32 - 'A' as u32,
            'a'..='z' => *字符 as u32 - 'a' as u32 + 26,
            '0'..='9' => *字符 as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("BASE64 数据包含非法字符 '{}'", 字符)),
        };
        缓冲 = (缓冲 << 6) | 值;
        位数 += 6;
        if 位数 >= 8 {
            位数 -= 8;
            结果.push((缓冲 >> 位数) as u8);
            缓冲 &= (1 << 位数) - 1;
        }
    }
    // 5) 结果必须是合法 UTF-8（智语言 文本 是 UTF-8 字符串，二进制内容不属本函数职责）
    match String::from_utf8(结果) {
        Ok(文本) => Ok(文本),
        Err(_) => Err("BASE64 解码结果不是合法 UTF-8 文本".to_string()),
    }
}
