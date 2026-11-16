// 文件: 项目管理/src/版本号.rs
// 职责: 语义化版本号（主.次.修订）解析、比较与「按约束选最佳版本」的**单一真相**。
//
// 为什么放在 pm：版本选择既被 `库管理`（安装/更新时选版本）使用，也被 `依赖解析`
// （运行时把 `导入 库名/模块路径` 解析到**唯一一个**已安装版本）使用。
// 原先实现只在 `库管理/src/库版本解析.rs`，pm 无法反向依赖库管理 ⇒ 依赖解析只能
// "取锁文件第一条" ⇒ 多版本共存时永远加载最旧版本（`库 更新` 后仍跑旧代码）。
// ⇒ 逻辑下沉到 pm，库管理侧改为薄转发（保持原路径 API 不变）。

/// 解析 `主.次.修订` 三段数字版本；不合法（段数不为 3 / 非数字）返回 None。
pub fn 解析版本(版本: &str) -> Option<(u32, u32, u32)> {
    let 部分: Vec<&str> = 版本.split('.').collect();
    if 部分.len() == 3 {
        let 主 = 部分[0].parse().ok()?;
        let 次 = 部分[1].parse().ok()?;
        let 修订 = 部分[2].parse().ok()?;
        Some((主, 次, 修订))
    } else {
        None
    }
}

/// 两版本比较（`a` 低 ⇒ Less）。不可解析者视为最低（`None`），且彼此相等。
pub fn 比较版本(a: &str, b: &str) -> std::cmp::Ordering {
    match (解析版本(a), 解析版本(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// 在可用版本集合中按约束选**最佳（最大）**版本。
/// 支持：`>= X`、`>= X, < Y`、精确 `X.Y.Z`。约束为空/`*` 视为 `>= 0.0.0`。
pub fn 选择最佳版本(可用版本: &[String], 约束: &str) -> Result<String, String> {
    let 约束 = 约束.trim();
    let 约束 = if 约束.is_empty() || 约束 == "*" { ">= 0.0.0" } else { 约束 };

    let 解析后版本: Vec<(u32, u32, u32)> = 可用版本
        .iter()
        .filter_map(|v| 解析版本(v))
        .collect();

    if 解析后版本.is_empty() {
        return Err("无可用版本".into());
    }

    if 约束.starts_with(">=") && 约束.contains(',') {
        let 部分: Vec<&str> = 约束.split(',').collect();
        let 下限 = 解析版本(部分[0].trim().trim_start_matches(">=").trim())
            .ok_or("无效下限版本约束")?;
        let 上限 = 解析版本(部分[1].trim().trim_start_matches('<').trim())
            .ok_or("无效上限版本约束")?;

        let mut 最佳: Option<(u32, u32, u32)> = None;
        for v in &解析后版本 {
            if v >= &下限 && v < &上限 {
                // 区间内取**最大**版本（不依赖可用版本集合的顺序）
                if 最佳.as_ref().map_or(true, |b| v > b) {
                    最佳 = Some(*v);
                }
            }
        }
        最佳
            .map(|v| format!("{}.{}.{}", v.0, v.1, v.2))
            .ok_or("无满足约束的版本".into())
    } else if 约束.starts_with(">=") {
        let 下限 = 解析版本(约束.trim_start_matches(">=").trim())
            .ok_or("无效版本约束")?;
        let 最佳 = 解析后版本.iter().max().ok_or("无可用版本")?;
        if 最佳 >= &下限 {
            Ok(format!("{}.{}.{}", 最佳.0, 最佳.1, 最佳.2))
        } else {
            Err("无满足约束的版本".into())
        }
    } else {
        解析版本(约束)
            .map(|v| format!("{}.{}.{}", v.0, v.1, v.2))
            .ok_or("无效版本号".into())
    }
}
