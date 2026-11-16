// 文件: 版本管理器/src/命令/参数解析.rs
// 职责: 共享参数解析工具，消除重复的参数匹配代码

/// 需要消耗一个值参数的标志列表
const 带值标志: &[&str] = &["--过滤", "--目标平台", "--重复", "--输出", "-o", "--超时"];

/// 从参数列表中提取第一个非标志的文件路径，
/// 跳过已知消耗值标志后面的值参数
pub fn 提取文件路径(参数: &[String]) -> Option<String> {
    for (i, 参) in 参数.iter().enumerate() {
        // 跳过 ./ 输出到当前目录标记（规范 §6.2.5）
        if 参 == "./" || 参 == ".\\" { continue; }
        // 跳过以 -- 或 - 开头的标志
        if 参.starts_with("--") || 参.starts_with('-') { continue; }
        // 跳过作为标志值的参数（如前一个是 --过滤 关键词）
        if i > 0 && 带值标志.contains(&参数[i - 1].as_str()) {
            continue;
        }
        return Some(参.clone());
    }
    None
}

/// 检测参数列表中是否包含输出到当前目录标记（./ 或 .\）
pub fn 提取输出目录标志(参数: &[String]) -> bool {
    参数.iter().any(|p| p == "./" || p == ".\\")
}

/// 检查参数列表中是否包含指定标志
pub fn 有标志(参数: &[String], 标志: &str) -> bool {
    参数.iter().any(|p| p == 标志)
}

/// 获取标志后面的值，如 --过滤 关键词 → "关键词"
pub fn 获取标志值(参数: &[String], 标志: &str) -> Option<String> {
    for i in 0..参数.len().saturating_sub(1) {
        if 参数[i] == 标志 {
            let 值 = &参数[i + 1];
            if !值.starts_with("--") {
                return Some(值.clone());
            }
        }
    }
    None
}


