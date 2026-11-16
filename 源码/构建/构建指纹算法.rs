// 文件: 源码/构建/构建指纹算法.rs
// 职责: 「源码树内容指纹」的唯一实现。**不是** lib 模块 —— 由各 build.rs 以 include! 引入。
//
// 为什么必须单源（2026-11-03）：
//   打包闸门要求「工具 exe 内嵌的基座指纹」与「base.lib 内嵌的构建标识」逐位相等。
//   两边各写一份实现，一旦有一边改了算法（哪怕只是排序规则），就会变成"永远不同源"的假警报。
//   项目里已因「两处各写一份魔数」踩过函数索引基址 300/400 的坑，故此处从一开始就单源。
//
// 为什么用 FNV-1a 而不是 std 的 DefaultHasher：
//   SipHash 的具体实现不在 Rust 的稳定性保证内。base 的 build.rs 与 版本管理器 的 build.rs
//   若由不同 rustc 版本编译，可能对同一份源码算出不同值 ⇒ 假不一致（且极难排查）。
//   FNV-1a 是纯整数算术（异或 + 乘法），任何工具链、任何平台结果恒定。
//
// 为什么用「相对路径」参与哈希：
//   绝对路径会让「仓库搬家 / 外层目录改名」等同于「源码改变」（历史上外层工作区确实改过名）。
//   相对路径 + 统一 `/` 分隔符 ⇒ 同一份内容在任何目录、任何平台都得到同一个指纹，
//   也让外部脚本能独立复算（可作为门禁断言）。

use std::path::{Path, PathBuf};

/// FNV-1a 64 位偏移基与质数（公开常量，任何实现都一致）。
const FNV_偏移基: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_质数: u64 = 0x0000_0100_0000_01b3;

fn 混合(状态: &mut u64, 字节: &[u8]) {
    for &b in 字节 {
        *状态 ^= b as u64;
        *状态 = 状态.wrapping_mul(FNV_质数);
    }
}

/// 递归收集目录下所有 `.rs` 文件（不跟随软链接目录，不记录空目录）。
fn 收集源文件(目录: &Path, 收集器: &mut Vec<PathBuf>) {
    let Ok(项) = std::fs::read_dir(目录) else { return };
    for 条目 in 项.flatten() {
        let 路径 = 条目.path();
        if 路径.is_dir() {
            收集源文件(&路径, 收集器);
        } else if 路径.extension().map(|e| e == "rs").unwrap_or(false) {
            收集器.push(路径);
        }
    }
}

/// 计算目录内容指纹：对 `(相对路径, 文件内容)` 按相对路径字节序排序后依次混合。
/// 返回 16 位小写十六进制字符串。
pub fn 计算目录指纹(根: &Path) -> String {
    let mut 文件列表 = Vec::new();
    收集源文件(根, &mut 文件列表);

    let mut 项: Vec<(String, PathBuf)> = 文件列表
        .into_iter()
        .filter_map(|完整| {
            let 相对 = 完整
                .strip_prefix(根)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((相对, 完整))
        })
        .collect();

    // 以字节序排序：不受文件系统枚举顺序与本地化比较规则影响
    项.sort_by(|甲, 乙| 甲.0.as_bytes().cmp(乙.0.as_bytes()));

    let mut 状态 = FNV_偏移基;
    for (相对, 完整) in 项 {
        混合(&mut 状态, 相对.as_bytes());
        混合(&mut 状态, b"\0"); // 边界分隔：避免「a」+「bc」与「ab」+「c」拼接碰撞
        if let Ok(内容) = std::fs::read(&完整) {
            混合(&mut 状态, &内容);
        }
        混合(&mut 状态, b"\0");
    }
    format!("{:016x}", 状态)
}
