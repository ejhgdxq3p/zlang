// 文件: 版本管理器/build.rs
// 职责: 在构建 智语言开发套件 时，对共享前端层（frontend / compiler / basic_functions / interpreter）
// 的 Rust 源树计算「内容指纹」，经 cargo:rustc-env 注入二进制。
// 运行时通过 env! 读取，用于驱动所有编译产物的增量缓存失效判定，
// 彻底消除「改前端忘了 bump 编译器逻辑版本 → 命中旧产物」这一类人工约定型 bug。
//
// 设计要点:
// 1) 指纹在【构建期】算好并烧进二进制，部署态无需源码（不依赖运行时哈希源树）。
// 2) 四条 cargo:rerun-if-changed 指向四个源树，改任一源文件必触发重算（否则指纹不刷新！）。
// 3) 细粒度指纹而非单一整体哈希: 改编译器 crate 只让编译模式失效、不误伤解释模式。
// 4) 算法单源：与 基础功能/build.rs 共用 源码/构建/构建指纹算法.rs（FNV-1a + 相对路径）。
//    两边必须逐位一致 —— 打包闸门要求「工具 exe 内嵌的基座指纹」==「base.lib 内嵌的构建标识」。
//    单源实现放这里而不是各写一份，是为了避免两处算法漂移导致"永远不同源"的假警报。

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../构建/构建指纹算法.rs"));

fn 主() {
    let 清单目录 = Path::new(env!("CARGO_MANIFEST_DIR"));
    // 相对仓库根: 版本管理器/../ 即 源码/ 根
    let 源码根 = 清单目录.parent().expect("无法由 CARGO_MANIFEST_DIR 推导 源码/ 根目录");

    let 前端源 = 源码根.join("前端").join("src");
    let 编译器源 = 源码根.join("后端").join("编译器").join("src");
    let 基础源 = 源码根.join("基础功能").join("src");
    let 解释器源 = 源码根.join("后端").join("解释器").join("src");

    let 前端指纹 = 计算目录指纹(&前端源);
    let 编译器指纹 = 计算目录指纹(&编译器源);
    let 基础指纹 = 计算目录指纹(&基础源);
    let 解释器指纹 = 计算目录指纹(&解释器源);

    println!("cargo:rustc-env=前端指纹={}", 前端指纹);
    println!("cargo:rustc-env=编译器指纹={}", 编译器指纹);
    println!("cargo:rustc-env=基础指纹={}", 基础指纹);
    println!("cargo:rustc-env=解释器指纹={}", 解释器指纹);

    // ⚠️ 关键: 改任一源树必须触发 build.rs 重算，否则指纹不会刷新
    println!("cargo:rerun-if-changed={}", 前端源.display());
    println!("cargo:rerun-if-changed={}", 编译器源.display());
    println!("cargo:rerun-if-changed={}", 基础源.display());
    println!("cargo:rerun-if-changed={}", 解释器源.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../构建/构建指纹算法.rs")
    );
}

fn main() {
    主();
}
