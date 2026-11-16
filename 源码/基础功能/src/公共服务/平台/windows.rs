// 文件: 基础功能/src/平台/windows.rs
// 职责: Windows 平台特有能力实现（被 mod.rs cfg 选中并 re-export）
#![allow(non_snake_case)]

pub fn 标识() -> &'static str { "windows" }
pub fn 名称() -> &'static str { "Windows" }

pub fn 库前缀() -> &'static str { "" }
pub fn 库后缀() -> &'static str { ".lib" }
pub fn 动态库前缀() -> &'static str { "" }
pub fn 动态库后缀() -> &'static str { ".dll" }

/// 外壳命令：(程序, 参数) —— Windows 用 cmd /C
pub fn 外壳() -> (&'static str, &'static str) {
    ("cmd", "/C")
}

/// 延迟删除目录：Windows **不允许删除正在运行的 exe**，
/// 故 `智语言 卸载全部` 只能把"删整个安装根"交给本进程退出后执行的外壳命令。
/// `ping -n 3` 是最轻量的等待（约 2 秒），不依赖 timeout.exe 这类可选组件。
pub fn 延迟删除目录命令(路径: &str) -> String {
    format!("ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{}\"", 路径)
}

/// 加载器路径：交给 LoadLibraryW 前加 verbatim 前缀（含 UNC、防双前缀）。
/// 这是突破 Windows MAX_PATH(260) 与代码页限制的唯一机制（已双模验证）。
/// ★2026-08-10 修复：verbatim 前缀 `\\?\` 要求内部全反斜杠，混合的 `/` 会被视为非法 →
/// LoadLibraryExW 返回 126（如 全局目录 路径经 join 产生 `\\?\C:/Users/.../产出\...` 混合分隔符）。
/// 故加前缀前先把所有 `/` 归一为 `\`（相对路径不处理，交给系统按当前目录解析）。
pub fn 加载器路径(路径: &str) -> String {
    let 归一 = 路径.replace('/', "\\");
    if std::path::Path::new(&归一).is_absolute() {
        if 归一.starts_with("\\\\?\\") {
            归一
        } else if 归一.starts_with("\\\\") {
            format!("\\\\?\\UNC\\{}", &归一[2..])
        } else {
            format!("\\\\?\\{}", 归一)
        }
    } else {
        归一
    }
}

/// 子进程路径：交给 rustc / 链接器 / 其它子进程前剥 verbatim 前缀 + 反斜杠归一化。
/// 子进程工具对带前缀路径处理异常，必须剥除（否则 .lib 打不开、符号丢失）。
pub fn 子进程路径(路径: &str) -> String {
    let 剥 = 路径.strip_prefix("\\\\?\\").unwrap_or(路径);
    // 链接器对混合分隔符敏感，统一为正斜杠（rustc / rust-lld 接受）
    剥.replace('\\', "/")
}

/// Windows 无 POSIX 权限位概念，no-op
pub fn 使可执行(_路径: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
