// 文件: 基础功能/src/平台/linux.rs
// 职责: Linux 平台特有能力实现（被 mod.rs cfg 选中并 re-export）
#![allow(non_snake_case)]

pub fn 标识() -> &'static str { "linux" }
pub fn 名称() -> &'static str { "Linux" }

pub fn 库前缀() -> &'static str { "lib" }
pub fn 库后缀() -> &'static str { ".a" }
pub fn 动态库前缀() -> &'static str { "lib" }
pub fn 动态库后缀() -> &'static str { ".so" }

/// 外壳命令：(程序, 参数) —— POSIX 用 sh -c
pub fn 外壳() -> (&'static str, &'static str) {
    ("sh", "-c")
}

/// 延迟删除目录：与 Windows 同语义（本进程退出后再删，避免与自身文件句柄冲突）。
/// POSIX 其实允许删除运行中的可执行文件，统一走这条路径以保证三平台行为一致。
pub fn 延迟删除目录命令(路径: &str) -> String {
    format!("sleep 2; rm -rf '{}'", 路径)
}

/// POSIX 路径即 UTF-8 字节串，无 MAX_PATH / 代码页问题，原样返回
pub fn 加载器路径(路径: &str) -> String {
    路径.to_string()
}

/// POSIX 子进程路径原样返回（verbatim 前缀在 POSIX 上不存在）
pub fn 子进程路径(路径: &str) -> String {
    路径.to_string()
}

/// POSIX：赋予 0o755 执行位（动态库 / 可执行脚本需要）
pub fn 使可执行(路径: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut 权限 = std::fs::metadata(路径)?.permissions();
    权限.set_mode(0o755);
    std::fs::set_permissions(路径, 权限)
}
