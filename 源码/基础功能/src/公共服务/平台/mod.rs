// 文件: 基础功能/src/平台/mod.rs
// 职责: 跨平台唯一真相层。业务代码禁止手写 cfg!(target_os)，一律引用本模块。
// 设计参照 Rust std::os 结构：共性 API 在 mod.rs，平台特有能力在各平台子模块实现，
// 由各平台文件全量实现并 re-export，业务层只可见统一签名（统一架构、双后端自动受益）。
#![allow(non_snake_case)]

#[cfg(target_os = "windows")] pub mod windows;
#[cfg(target_os = "linux")]   pub mod linux;
#[cfg(target_os = "macos")]   pub mod macos;

#[cfg(target_os = "windows")] pub use windows::*;
#[cfg(target_os = "linux")]   pub use linux::*;
#[cfg(target_os = "macos")]   pub use macos::*;

// ── 跨平台共享（不随 OS 变化的常量 / 逻辑，定义一次）──

/// 当前 CPU 架构字符串（x64 / aarch64 / x86 / unknown）
pub fn 架构() -> &'static str {
    #[cfg(target_arch = "x86_64")] { "x64" }
    #[cfg(target_arch = "aarch64")] { "aarch64" }
    #[cfg(target_arch = "x86")] { "x86" }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))] { "unknown" }
}

/// 全标识（如 windows-x64 / linux-x64 / macos-aarch64），用于工具链 / 安装包命名
pub fn 全标识() -> String {
    format!("{}-{}", 标识(), 架构())
}

/// 可执行文件名：基名 + 平台后缀（Windows .exe，其余无后缀）
pub fn 可执行文件名(基名: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", 基名)
    } else {
        基名.to_string()
    }
}

/// 系统级安装路径（规范 6.1.5 首次安装选项2「系统目录」，需要管理员权限）
///   Windows : %ProgramFiles%\智语言
///   Linux   : /usr/local/bin/智语言
///   macOS   : /usr/local/bin/智语言
pub fn 系统安装路径() -> std::path::PathBuf {
    if cfg!(windows) {
        let 基 = std::env::var("ProgramFiles")
            .unwrap_or_else(|_| r"C:\Program Files".to_string());
        std::path::PathBuf::from(基).join("智语言")
    } else {
        std::path::PathBuf::from("/usr/local/bin").join("智语言")
    }
}

/// 兼容路径：把用户输入中的 / 与 \ 统一为当前平台分隔符（库名解析 / 配置读取用）
pub fn 兼容路径(路径: &str) -> String {
    if cfg!(windows) {
        路径.replace('/', "\\")
    } else {
        路径.replace('\\', "/")
    }
}
