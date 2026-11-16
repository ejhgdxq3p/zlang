// 产物路径.rs — 规范 6.2.4 输出路径规则
// 优先级：命令行指定 > 项目.配置[产出文件目录] > 全局默认

use std::path::{Path, PathBuf};
use crate::目录;

/// 项目输出配置（从 项目.配置 [产出文件目录] 段解析，缺失用默认值）
#[derive(Debug, Clone)]
pub struct 项目输出配置 {
    pub 源码字节码文件: PathBuf,
    pub 源码编译文件: PathBuf,
    pub 执行文件: PathBuf,
    pub 项目库字节码文件: PathBuf,
    pub 项目库编译文件: PathBuf,
    pub 全局库字节码文件: PathBuf,
    pub 全局库编译文件: PathBuf,
    /// 外部语言产物目录（规范 §6.2.1）
    pub 外部语言文件: PathBuf,   // 产出/外部语言文件/
    /// 导出产物基目录（规范 §6.2.1/§6.2.4）
    pub 导出文件: PathBuf,       // 产出/导出文件/
    /// 库封装产物目录（规范 §6.2.1）
    pub 封装文件: PathBuf,       // 产出/封装文件/
    /// FFI: 导出编译中间文件（基于 导出文件 拼接，落 产出/导出文件/编译文件/）
    pub 导出编译文件: PathBuf,   // 产出/导出文件/编译文件/
    /// FFI: 导出执行文件（基于 导出文件 拼接，落 产出/导出文件/执行文件/）
    pub 导出执行文件: PathBuf,   // 产出/导出文件/执行文件/
}

/// 从 项目.配置 [产出文件目录] 段解析输出路径。缺失字段用默认值。
/// 命令行指定目录优先级最高（传入非 None 时覆盖对应的项目配置路径）。
pub fn 解析项目输出配置(项目根: &Path) -> 项目输出配置 {
    let 配置内容 = 读项目配置文本(项目根);
    let 取 = |键: &str, 默认: &str| -> String {
        配置内容.iter()
            .find(|(k, _)| k == 键)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| 默认.to_string())
    };
    项目输出配置 {
        源码字节码文件: 项目根.join(取("源码字节码文件", "产出/字节码文件/")),
        源码编译文件:   项目根.join(取("源码编译文件", "产出/编译文件/")),
        执行文件:       项目根.join(取("执行文件", "产出/执行文件/")),
        项目库字节码文件: 项目根.join(取("项目库字节码文件", "产出/库中间文件/字节码文件/")),
        项目库编译文件:   项目根.join(取("项目库编译文件", "产出/库中间文件/编译文件/")),
        外部语言文件:     项目根.join(取("外部语言文件", "产出/外部语言文件/")),
        导出文件:         项目根.join(取("导出文件", "产出/导出文件/")),
        封装文件:         项目根.join(取("封装文件", "产出/封装文件/")),
        导出编译文件:     项目根.join(取("导出文件", "产出/导出文件/")).join("编译文件"),
        导出执行文件:     项目根.join(取("导出文件", "产出/导出文件/")).join("执行文件"),
        全局库字节码文件: {
            let v = 取("全局库字节码文件", "");
            if v.is_empty() {
                // 默认：全局目录/产出/库中间文件/字节码文件/
                目录::全局库中间目录().join("字节码文件")
            } else if PathBuf::from(&v).is_absolute() {
                // 绝对路径直接读取
                PathBuf::from(&v)
            } else {
                // 相对路径拼接方式为 项目目录+路径
                项目根.join(v)
            }
        },
        全局库编译文件: {
            let v = 取("全局库编译文件", "");
            if v.is_empty() {
                // 默认：全局目录/产出/库中间文件/编译文件/
                目录::全局库中间目录().join("编译文件")
            } else if PathBuf::from(&v).is_absolute() {
                PathBuf::from(&v)
            } else {
                项目根.join(v)
            }
        },
    }
}

/// 使用配置读取器解析 [产出文件目录] 段 → Vec<(键, 值)>
fn 读项目配置文本(项目根: &Path) -> Vec<(String, String)> {
    let 路径 = 项目根.join("配置").join("项目.配置");
    let 文档 = match crate::配置读取器::解析文件(&路径) {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    if let Some(段) = crate::配置读取器::取段(&文档, "产出文件目录") {
        段.键值.iter().filter_map(|(k, v)| {
            match v {
                crate::配置读取器::配置值::文本(s) => Some((k.clone(), s.clone())),
                _ => {
                    // 非文本值序列化为字符串
                    let mut buf = String::new();
                    crate::配置读取器::写值(&mut buf, v);
                    Some((k.clone(), buf.trim_matches('"').to_string()))
                }
            }
        }).collect()
    } else {
        vec![]
    }
}

// ======================== 项目源码产物 ========================

/// 字节码产物路径（项目内保持源码目录结构，项目外放全局）
pub fn 字节码产物路径(源文件: &Path, 项目根: Option<&Path>) -> PathBuf {
    match 项目根 {
        Some(根) => {
            let 配置 = 解析项目输出配置(根);
            源码路径(源文件, 根, &配置.源码字节码文件, ".字节码")
        }
        None => {
            let 主干 = 源文件.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let 目录 = 目录::全局字节码目录();
            std::fs::create_dir_all(&目录).ok();
            目录.join(format!("{}.字节码", 主干))
        }
    }
}

/// 执行文件产物路径（项目内保持源码目录结构）
pub fn 执行产物路径(源文件: &Path, 项目根: &Path, 后缀: &str) -> PathBuf {
    let 配置 = 解析项目输出配置(项目根);
    源码路径(源文件, 项目根, &配置.执行文件, 后缀)
}

/// 编译中间文件产物路径（项目内保持源码目录结构）
pub fn 编译产物路径(源文件: &Path, 项目根: &Path) -> PathBuf {
    let 配置 = 解析项目输出配置(项目根);
    源码路径(源文件, 项目根, &配置.源码编译文件, ".编译")
}

/// 项目源码通用路径拼接（规范 6.2.4 相对路径提取：移除 源码/ 前缀，保留后续目录结构）
/// 例：源码/模型/用户.智 → 相对路径 模型/用户 → <基础目录>/模型/用户.字节码
fn 源码路径(源文件: &Path, 项目根: &Path, 基础目录: &Path, 后缀: &str) -> PathBuf {
    let 相对 = 文件相对项目路径(源文件, 项目根);
    // 移除前导 源码/ 组件（规范 6.2.4：路径层级在产物目录下镜像保留，但不含 源码/ 一层）
    let 剥离 = 相对.strip_prefix("源码").unwrap_or(&相对);
    let 主干 = 剥离.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let 父 = 剥离.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let 目标目录 = 基础目录.join(父);
    std::fs::create_dir_all(&目标目录).ok();
    目标目录.join(format!("{}{}", 主干, 后缀))
}

// ======================== 库产物 ========================

/// 库字节码路径：<基础路径>/<库名><版本号>/<模块路径>.字节码
pub fn 库字节码路径(基础路径: &Path, 库名: &str, 版本: &str, 模块: &str) -> PathBuf {
    库路径(基础路径, 库名, 版本, 模块, ".字节码")
}

/// 库编译路径：<基础路径>/<库名><版本号>/<模块路径>.编译
pub fn 库编译路径(基础路径: &Path, 库名: &str, 版本: &str, 模块: &str) -> PathBuf {
    库路径(基础路径, 库名, 版本, 模块, ".编译")
}

fn 库路径(基础路径: &Path, 库名: &str, 版本: &str, 模块: &str, 后缀: &str) -> PathBuf {
    let 库前缀 = format!("{}{}", 库名, 版本);
    let 目录 = 基础路径.join(库前缀);
    std::fs::create_dir_all(&目录).ok();
    // 模块路径 = 库内从 源码/ 开始的相对路径去掉 .智 后缀（保留子目录）
    // 例：模块 "工具" 或 "工具.智" → "工具"；"几何/圆形.智" → "几何/圆形"
    let 主干 = Path::new(模块).with_extension("").to_string_lossy().to_string();
    目录.join(format!("{}{}", 主干, 后缀))
}

// ======================== 工具函数 ========================

/// 计算文件相对项目根的路径
pub fn 文件相对项目路径(文件: &Path, 根: &Path) -> PathBuf {
    if let (Ok(完整), Ok(根规范)) = (文件.canonicalize(), 根.canonicalize()) {
        if let Ok(相对) = 完整.strip_prefix(&根规范) { return 相对.to_path_buf(); }
    }
    文件.to_path_buf()
}

#[cfg(test)]
mod 全局库路径测试 {
    use super::*;
    use std::fs;

    fn 写项目配置(根: &Path, 内容: &str) {
        fs::create_dir_all(根.join("配置")).ok();
        fs::write(根.join("配置").join("项目.配置"), 内容).ok();
    }

    #[test]
    fn 全局库路径_未配置走全局目录默认() {
        let 根 = std::env::temp_dir().join("pm_path_default");
        fs::create_dir_all(&根).ok();
        写项目配置(&根, "[项目]\n名称 = \"x\"\n版本 = \"0.1.0\"\n");
        let 配置 = 解析项目输出配置(&根);
        assert_eq!(配置.全局库字节码文件, 目录::全局库中间目录().join("字节码文件"));
        assert_eq!(配置.全局库编译文件, 目录::全局库中间目录().join("编译文件"));
        fs::remove_dir_all(&根).ok();
    }

    #[test]
    fn 全局库路径_相对按项目目录拼接() {
        let 根 = std::env::temp_dir().join("pm_path_rel");
        fs::create_dir_all(&根).ok();
        let 内容 = "[项目]\n名称 = \"x\"\n版本 = \"0.1.0\"\n[产出文件目录]\n全局库字节码文件 = \"产出/全局库中间文件/字节码文件/\"\n全局库编译文件 = \"产出/全局库中间文件/编译文件/\"\n";
        写项目配置(&根, 内容);
        let 配置 = 解析项目输出配置(&根);
        assert_eq!(配置.全局库字节码文件, 根.join("产出/全局库中间文件/字节码文件/"));
        assert_eq!(配置.全局库编译文件, 根.join("产出/全局库中间文件/编译文件/"));
        fs::remove_dir_all(&根).ok();
    }

    #[test]
    fn 全局库路径_绝对路径直接读取() {
        let 根 = std::env::temp_dir().join("pm_path_abs");
        fs::create_dir_all(&根).ok();
        let abs_b = std::env::temp_dir().join("pm_abs_字节码");
        let abs_c = std::env::temp_dir().join("pm_abs_编译");
        let 内容 = format!(
            "[项目]\n名称 = \"x\"\n版本 = \"0.1.0\"\n[产出文件目录]\n全局库字节码文件 = \"{}\"\n全局库编译文件 = \"{}\"\n",
            abs_b.display(), abs_c.display()
        );
        写项目配置(&根, &内容);
        let 配置 = 解析项目输出配置(&根);
        assert!(配置.全局库字节码文件.is_absolute(), "全局库字节码文件 应为绝对路径");
        assert!(配置.全局库编译文件.is_absolute(), "全局库编译文件 应为绝对路径");
        assert_eq!(配置.全局库字节码文件, abs_b);
        assert_eq!(配置.全局库编译文件, abs_c);
        fs::remove_dir_all(&根).ok();
    }
}
