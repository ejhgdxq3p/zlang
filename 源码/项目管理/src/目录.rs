// 文件: 工程管理/src/目录.rs
// 职责: 统一的全局/项目目录真相源（双模式共享）
// 规范 6.1.2 / 6.1.8：全局目录默认根路径与各操作系统映射

use std::path::{Path, PathBuf};

/// 全局目录（单一真相）：
///   Linux    ~/.智语言/
///   macOS    ~/Library/Application Support/智语言/
///   Windows  %LOCALAPPDATA%/智语言/
pub fn 全局目录() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = 家目录() {
            return home.join(".智语言");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = 家目录() {
            return home
                .join("Library")
                .join("Application Support")
                .join("智语言");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join(".智语言");
        }
    }
    // 兜底（无配套操作系统宏或环境变量缺失）
    if let Some(home) = 家目录() {
        return home.join(".智语言");
    }
    兜底基目录().join(".智语言")
}

/// 兜底基目录（**绝不用相对路径**）：环境变量全缺时，退到"当前工作目录"的**绝对**形式。
///
/// 为什么必须绝对：相对路径按**进程 cwd** 解析 ⇒ 同一程序在不同工作目录下会把
/// "全局目录 / 安装目录"落到不同位置（本项目实测过的"产物落到工作区根"即同类问题）；
/// 绝对化后落点唯一、可打印、可断言 ✓。
fn 兜底基目录() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn 家目录() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// 安装路径（开发套件安装根目录）
/// 规范 6.1.4：优先读 全局.配置 [开发套件].安装路径，否则按各操作系统映射默认路径。
pub fn 安装路径() -> PathBuf {
    let 配置 = crate::全局配置::读取配置();
    if !配置.开发套件.安装路径.is_empty() {
        let p = PathBuf::from(&配置.开发套件.安装路径);
        if p.is_absolute() {
            return p;
        }
        // ★ 2026-11-03 防御：历史配置里可能被旧版安装器写入**相对路径**
        //   （旧版把用户参数原样落盘）。相对路径按「进程当前目录」解析会位置漂移，
        //   这里统一按当前目录绝对化，至少保证「同一时刻」解析结果稳定、可见。
        return std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p);
    }
    默认安装路径()
}

/// 安装路径的默认映射（不读取配置，避免与 读取配置 形成递归）
pub fn 默认安装路径() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        if let Some(h) = 家目录() {
            return h.join(".local").join("bin").join("智语言");
        }
        return 兜底基目录().join(".local").join("bin").join("智语言");
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = 家目录() {
            return h.join("Library")
                .join("Application Support")
                .join("智语言");
        }
        return 兜底基目录().join("Library").join("Application Support").join("智语言");
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("Programs").join("智语言");
        }
        return 兜底基目录().join("Programs").join("智语言");
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        全局目录()
    }
}

/// 开发套件下载目录（.库/zip 等安装包的临时存放，规范 6.1.2 下载/开发套件文件）
pub fn 开发套件下载目录() -> PathBuf {
    全局目录().join("下载").join("开发套件文件")
}

/// 指定版本的开发套件目录：安装路径/版本<版本>（规范 6.1.1）
pub fn 版本目录(版本: &str) -> PathBuf {
    安装路径().join(format!("版本{}", 版本))
}

/// 指定版本的开发套件可执行文件：版本目录/执行文件/智语言(.exe)
/// 跨平台：可执行后缀由 基础功能::平台 统一提供（Windows .exe，POSIX 无后缀）
pub fn 版本可执行文件(版本: &str) -> PathBuf {
    版本目录(版本).join("执行文件").join(base::平台::可执行文件名("智语言"))
}

/// 版本管理器所在目录：安装路径/版本管理器（加入 PATH 的目录，规范 6.1.1）
pub fn 引导程序目录() -> PathBuf {
    安装路径().join("版本管理器")
}

/// 版本管理器可执行文件：安装路径/版本管理器/智语言(.exe)
/// 单例：首次安装写入，后续版本安装检测到已存在则跳过
/// 跨平台：可执行后缀由 基础功能::平台 统一提供
pub fn 引导程序可执行文件() -> PathBuf {
    引导程序目录().join(base::平台::可执行文件名("智语言"))
}

/// 扫描已安装的开发套件版本（读取安装路径下所有 版本<版本号> 目录）
/// 显式排除 版本管理器 目录，并校验目录名为语义化版本格式，避免误扫。
pub fn 扫描已安装版本() -> Vec<String> {
    let mut 版本列表 = Vec::new();
    let 根 = 安装路径();
    if let Ok(条目) = std::fs::read_dir(&根) {
        for 条目 in 条目.flatten() {
            let 名称 = 条目.file_name().to_string_lossy().to_string();
            if 名称 == "版本管理器" {
                continue;
            }
            if let Some(号) = 名称.strip_prefix("版本") {
                if 条目.path().is_dir() && 是语义化版本号(&号) {
                    版本列表.push(号.to_string());
                }
            }
        }
    }
    版本列表.sort();
    版本列表
}

/// 判断是否为 "主.次.修订" 纯数字语义化版本号
fn 是语义化版本号(文本: &str) -> bool {
    let 部分: Vec<&str> = 文本.split('.').collect();
    部分.len() == 3
        && 部分.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// 标准库根（双模式共享真相源）
/// 标准库仅以源码（.智）形态安装（2026-08-03 定稿：去除 字节码文件/编译文件 两份，
/// 运行时由内建 native 后端 + 预注册符号零消费，文件仅作 API 门面参考）。
/// 开发环境（仓库内存在 源码/标准库）直接返回 源码/标准库 源目录。
pub fn 标准库根(_编译模式: bool) -> PathBuf {
    // 开发环境：仓库内的 源码/标准库 优先（源码即真相，双模式一致）
    let 开发根 = PathBuf::from("源码/标准库");
    if 开发根.exists() {
        return 开发根;
    }
    // 已安装工具包：标准库/ 直接放 .智 源码
    let 激活版 = crate::全局配置::读取配置().开发套件.开发套件版本;
    版本目录(&激活版).join("标准库")
}

/// 标准库源码根（安装后标准库下的 .智 源码文件位置，与 标准库根 同目录）
/// 开发环境直接返回 源码/标准库，安装环境返回 版本目录/标准库
pub fn 标准库源码根() -> PathBuf {
    let 开发根 = PathBuf::from("源码/标准库");
    if 开发根.exists() {
        return 开发根;
    }
    let 激活版 = crate::全局配置::读取配置().开发套件.开发套件版本;
    版本目录(&激活版).join("标准库")
}

/// 全局配置目录
pub fn 全局配置目录() -> PathBuf {
    全局目录().join("配置")
}

/// 全局.配置 文件路径
pub fn 全局配置文件() -> PathBuf {
    全局配置目录().join("全局.配置")
}

/// 全局库安装根目录
pub fn 全局库根() -> PathBuf {
    全局目录().join("库")
}

/// 全局字节码产物目录（单文件解释执行 + 全局库字节码）
pub fn 全局字节码目录() -> PathBuf {
    全局目录().join("产出").join("字节码文件")
}

/// 全局编译中间文件目录
pub fn 全局编译目录() -> PathBuf {
    全局目录().join("产出").join("编译文件")
}

/// 全局可执行文件目录（单文件构建产物）
pub fn 全局执行目录() -> PathBuf {
    全局目录().join("产出").join("执行文件")
}

/// 全局库中间文件目录（按 <库名><版本号>/ 组织）
pub fn 全局库中间目录() -> PathBuf {
    全局目录().join("产出").join("库中间文件")
}

/// 项目级字节码产物目录
pub fn 项目字节码目录(项目根: &Path) -> PathBuf {
    项目根.join("产出").join("字节码文件")
}

/// 项目级编译中间文件目录
pub fn 项目编译目录(项目根: &Path) -> PathBuf {
    项目根.join("产出").join("编译文件")
}

/// 项目级可执行文件目录
pub fn 项目执行目录(项目根: &Path) -> PathBuf {
    项目根.join("产出").join("执行文件")
}

/// 项目级库中间文件目录
pub fn 项目库中间目录(项目根: &Path) -> PathBuf {
    项目根.join("产出").join("库中间文件")
}
