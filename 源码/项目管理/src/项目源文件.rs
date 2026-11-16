// 文件: 项目管理/src/项目源文件.rs
// 职责: 项目源文件扫描、入口查找、路径计算（双模式共享）
//       消除运行命令/编译命令间的重复实现

use std::fs;
use std::path::{Path, PathBuf};

/// 收集项目内 源码/ 目录下的所有 .智 源文件
pub fn 收集(项目根: &Path) -> Vec<String> {
    let mut 文件列表 = Vec::new();
    let 源码目录 = 项目根.join("源码");
    if 源码目录.exists() {
        递归收集(&源码目录, &mut 文件列表);
    }
    文件列表
}

/// 收集 源码/ 目录 + 项目根目录下的所有 .智 源文件
pub fn 收集含根目录(项目根: &Path) -> Vec<String> {
    let mut 文件列表 = 收集(项目根);
    // 也搜集根目录的 .智 文件
    if let Ok(条目) = fs::read_dir(项目根) {
        for 条目 in 条目.flatten() {
            let 路径 = 条目.path();
            if 路径.extension().map_or(false, |e| e == "智") {
                if let Some(s) = 路径.to_str() {
                    文件列表.push(s.to_string());
                }
            }
        }
    }
    文件列表
}

/// 递归收集 .智 源文件
fn 递归收集(目录: &Path, 结果: &mut Vec<String>) {
    if let Ok(条目) = fs::read_dir(目录) {
        for 条目 in 条目.flatten() {
            let 路径 = 条目.path();
            if 路径.is_dir() {
                递归收集(&路径, 结果);
            } else if 路径.extension().map_or(false, |e| e == "智") {
                if let Some(s) = 路径.to_str() {
                    结果.push(s.to_string());
                }
            }
        }
    }
}

/// 查找项目入口文件
/// 优先精确匹配 源码/启动程序.智，再递归搜索内容含"启动程序"的文件
pub fn 查找入口(项目根: &Path) -> Option<String> {
    let 源码目录 = 项目根.join("源码");
    let 候选文件 = 源码目录.join("启动程序.智");
    if 候选文件.exists() {
        return Some(候选文件.to_string_lossy().to_string());
    }
    // 递归搜索
    let mut 文件列表 = Vec::new();
    递归收集(&源码目录, &mut 文件列表);
    for 文件 in &文件列表 {
        if let Ok(内容) = fs::read_to_string(文件) {
            if 内容.contains("启动程序") {
                return Some(文件.clone());
            }
        }
    }
    None
}

/// 计算文件相对项目根的产物路径（canonicalize + strip_prefix + 剥离 源码/ 前缀）。
/// 规范 6.2.4：相对路径提取 = 移除 源码/ 前缀和 .智 后缀，保留后面的目录结构。
/// 例：源码/模型/用户.智 → 模型/用户.智（产物基目录拼接后为 模型/用户.字节码）
pub fn 计算相对路径(文件路径: &str, 项目根: &Path) -> PathBuf {
    if let Ok(完整) = PathBuf::from(文件路径).canonicalize() {
        if let Ok(根规范) = 项目根.canonicalize() {
            match 完整.strip_prefix(&根规范) {
                Ok(p) => {
                    // 移除前导 源码/ 组件（若相对路径以 源码 开头）
                    return p.strip_prefix("源码").unwrap_or(&p).to_path_buf();
                }
                Err(_) => return PathBuf::from(文件路径),
            }
        }
    }
    PathBuf::from(文件路径)
}
