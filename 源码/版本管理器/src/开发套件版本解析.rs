// 文件: 版本管理器/src/开发套件版本解析.rs
// 职责: 语义化版本解析、约束满足、最佳版本选择（规范 6.1.5 版本命名）

/// 解析 "主.次.修订" 为三元组；失败返回 None
pub fn 解析版本(版本: &str) -> Option<(u32, u32, u32)> {
    let 部分: Vec<&str> = 版本.trim().split('.').collect();
    if 部分.len() != 3 {
        return None;
    }
    let 主 = 部分[0].parse::<u32>().ok()?;
    let 次 = 部分[1].parse::<u32>().ok()?;
    let 修订 = 部分[2].parse::<u32>().ok()?;
    Some((主, 次, 修订))
}

/// 将三元组格式化为 "主.次.修订"
pub fn 格式版本(v: (u32, u32, u32)) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

/// 判断已解析版本是否满足约束字符串
/// 支持：精确(主.次.修订) / >=x / >x / <=x / <x / >=x,<y 区间 /
///       x.y.* 通配 / ^x.y.z 脱字号 / ~x.y.z 波浪号
pub fn 约束满足(版本: (u32, u32, u32), 约束: &str) -> bool {
    let 约束 = 约束.trim();

    // 区间：>=x,<y
    if 约束.contains(',') && 约束.contains(">=") {
        let 部分: Vec<&str> = 约束.split(',').collect();
        if 部分.len() != 2 {
            return false;
        }
        let 下限 = 部分[0].trim().trim_start_matches(">=").trim();
        let 上限 = 部分[1].trim().trim_start_matches('<').trim();
        let (Some(下), Some(上)) = (解析版本(下限), 解析版本(上限)) else {
            return false;
        };
        return 版本 >= 下 && 版本 < 上;
    }

    // 通配 x.y.*
    if let Some(前缀) = 约束.strip_suffix(".*") {
        if let Some((主, 次, _)) = 解析版本(&格式_三位前缀(前缀)) {
            let (p主, p次, _) = (主, 次, 0u32);
            let _ = p次;
            return 版本.0 == p主 && 版本.1 == p次;
        }
        return false;
    }

    // 脱字号 ^x.y.z  →  >=x.y.z, <(x+1).0.0
    if let Some(基) = 约束.strip_prefix('^') {
        if let Some(b) = 解析版本(基) {
            return 版本 >= b && 版本 < (b.0 + 1, 0, 0);
        }
        return false;
    }

    // 波浪号 ~x.y.z  →  >=x.y.z, <x.(y+1).0
    if let Some(基) = 约束.strip_prefix('~') {
        if let Some(b) = 解析版本(基) {
            return 版本 >= b && 版本 < (b.0, b.1 + 1, 0);
        }
        return false;
    }

    // 仅下限
    if let Some(下限) = 约束.strip_prefix(">=") {
        return match 解析版本(下限.trim()) {
            Some(v) => 版本 >= v,
            None => false,
        };
    }
    if let Some(下限) = 约束.strip_prefix('>') {
        return match 解析版本(下限.trim()) {
            Some(v) => 版本 > v,
            None => false,
        };
    }
    if let Some(上限) = 约束.strip_prefix("<=") {
        return match 解析版本(上限.trim()) {
            Some(v) => 版本 <= v,
            None => false,
        };
    }
    if let Some(上限) = 约束.strip_prefix('<') {
        return match 解析版本(上限.trim()) {
            Some(v) => 版本 < v,
            None => false,
        };
    }

    // 精确匹配
    match 解析版本(约束) {
        Some(v) => 版本 == v,
        None => false,
    }
}

/// 工具：把 "x.y" 或 "x" 前缀补成可解析的三元组字符串
fn 格式_三位前缀(前缀: &str) -> String {
    let 部分: Vec<&str> = 前缀.split('.').collect();
    match 部分.len() {
        1 => format!("{}.0.0", 部分[0]),
        2 => format!("{}.{}.0", 部分[0], 部分[1]),
        _ => 前缀.to_string(),
    }
}

/// 从可用版本中按约束选择最佳（最大）版本
/// 修复旧实现"取最后一个匹配"的 bug，改为取最大值。
pub fn 选择最佳版本(可用版本: &[String], 约束: &str) -> Result<String, String> {
    let 满足: Vec<(u32, u32, u32)> = 可用版本
        .iter()
        .filter_map(|v| 解析版本(v))
        .filter(|v| 约束满足(*v, 约束))
        .collect();
    if 满足.is_empty() {
        return Err(format!("无满足约束 \"{}\" 的可用版本", 约束));
    }
    满足
        .into_iter()
        .max()
        .map(格式版本)
        .ok_or_else(|| "无满足约束的版本".to_string())
}

/// 从远程可用版本中按约束选择最佳（最大）版本（同 选择最佳版本）
pub fn 选择远程最佳版本(可用版本: &[String], 约束: &str) -> Result<String, String> {
    选择最佳版本(可用版本, 约束)
}

/// 当前平台标识（规范 6.1.6）：linux-x64 / windows-x64 / macos-x64 等
pub fn 平台标识() -> String {
    let 操作系统 = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };
    let 架构 = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    };
    format!("{}-{}", 操作系统, 架构)
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 解析版本_正常() {
        assert_eq!(解析版本("0.3.0"), Some((0, 3, 0)));
        assert_eq!(解析版本("1.2.3"), Some((1, 2, 3)));
        assert_eq!(解析版本("1.2"), None);
        assert_eq!(解析版本("abc"), None);
    }

    #[test]
    fn 约束满足_精确() {
        let v = 解析版本("1.2.3").unwrap();
        assert!(约束满足(v, "1.2.3"));
        assert!(!约束满足(v, "1.2.4"));
    }

    #[test]
    fn 约束满足_下限() {
        let v = 解析版本("1.5.0").unwrap();
        assert!(约束满足(v, ">=1.0.0"));
        assert!(约束满足(v, ">1.0.0"));
        assert!(!约束满足(v, ">1.5.0"));
        assert!(约束满足(v, ">=1.5.0"));
    }

    #[test]
    fn 约束满足_上限() {
        let v = 解析版本("1.5.0").unwrap();
        assert!(约束满足(v, "<2.0.0"));
        assert!(约束满足(v, "<=1.5.0"));
        assert!(!约束满足(v, "<1.5.0"));
    }

    #[test]
    fn 约束满足_区间() {
        let v = 解析版本("1.5.0").unwrap();
        assert!(约束满足(v, ">=1.0.0,<2.0.0"));
        assert!(!约束满足(v, ">=2.0.0,<3.0.0"));
    }

    #[test]
    fn 约束满足_通配() {
        let v = 解析版本("1.2.9").unwrap();
        assert!(约束满足(v, "1.2.*"));
        assert!(!约束满足(v, "1.3.*"));
    }

    #[test]
    fn 约束满足_脱字号() {
        let v = 解析版本("1.9.9").unwrap();
        assert!(约束满足(v, "^1.2.0"));
        assert!(!约束满足(v, "^2.0.0"));
    }

    #[test]
    fn 选择最佳版本_取最大() {
        let 可用 = vec![
            "0.3.0".to_string(),
            "0.3.1".to_string(),
            "0.2.9".to_string(),
            "1.0.0".to_string(),
        ];
        assert_eq!(选择最佳版本(&可用, ">=0.0.0").unwrap(), "1.0.0");
        assert_eq!(选择最佳版本(&可用, ">=0.3.0,<1.0.0").unwrap(), "0.3.1");
        assert!(选择最佳版本(&可用, ">=2.0.0").is_err());
    }

    #[test]
    fn 平台标识_非空() {
        assert!(!平台标识().is_empty());
    }
}
