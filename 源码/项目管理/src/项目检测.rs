// 文件: 项目管理/src/项目检测.rs
// 职责: 项目根目录检测（向上查找 配置/项目.配置）
//       双模式共享：解释模式（虚拟机/模块加载器）与编译模式（代码生成）都经本模块
//       获取项目归属标识，保证导入解析语义跨模式一致

use std::path::{Path, PathBuf};

/// 项目归属枚举
pub enum 项目归属 {
    项目内(PathBuf), // 携带项目根目录路径
    项目外,
}

/// 从当前目录向上查找项目根目录（用于无参数命令）
/// 返回 项目内(根目录) 或 项目外
pub fn 检测当前目录项目归属() -> 项目归属 {
    let 当前目录 = match std::env::current_dir() {
        Ok(d) => d.canonicalize().unwrap_or(d),
        Err(_) => return 项目归属::项目外,
    };
    向上查找项目根(&当前目录)
}

/// 从指定文件路径向上查找项目根目录
/// 返回包含 `配置/项目.配置` 的目录，找不到返回 None
pub fn 查找项目根目录(文件路径: &str) -> Option<PathBuf> {
    let 路径 = Path::new(文件路径);
    let 起始目录 = if 路径.is_absolute() {
        路径.parent().map(|p| p.to_path_buf())
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| {
                let 完整 = cwd.join(文件路径);
                完整.parent().map(|p| p.to_path_buf())
            })
    };

    let 起始目录 = 起始目录?;
    let 规范起始 = 起始目录.canonicalize().unwrap_or(起始目录);
    向上查找(规范起始)
}

/// 从指定目录向上逐级查找项目根目录
fn 向上查找(起始目录: PathBuf) -> Option<PathBuf> {
    let mut 当前 = Some(起始目录.as_path());
    while let Some(目录) = 当前 {
        if 目录.join("配置").join("项目.配置").exists() {
            return Some(目录.to_path_buf());
        }
        当前 = 目录.parent();
    }
    None
}

fn 向上查找项目根(起始目录: &Path) -> 项目归属 {
    let mut 当前 = Some(起始目录);
    while let Some(目录) = 当前 {
        if 目录.join("配置").join("项目.配置").exists() {
            return 项目归属::项目内(目录.to_path_buf());
        }
        当前 = 目录.parent();
    }
    项目归属::项目外
}
