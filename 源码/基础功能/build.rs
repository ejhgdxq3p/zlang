// 文件: 基础功能/build.rs
// 职责: 构建 base（运行时静态库）时计算 源码/基础功能/src 的内容指纹，注入编译期常量，
//       使 base.lib 自身带着「我是由哪份源码构建的」（消费方见 运行时/构建标识.rs）。
//
// 为什么需要（2026-11-03）：
//   此前「base.lib 与工具是否同源」靠 产出/release/.base_build_fingerprint 这个**外部标记文件**
//   记录，而它只由 链接器::构建运行时静态库 在重建成功后写 ⇒ 任何人直接
//   `cargo build -p basic_functions` 都会得到「新库 + 旧标记」⇒ 打包闸门假失败（实测踩过）。
//   把身份编进产物后，判定不再需要任何人「记得同步」。
//
// ⚠ 本脚本绝不能 panic：base 是全仓最底层的 crate，脚本一挂就是整个项目构建不了。
//   所有路径/IO 失败都降级为「未知」并打警告（未标注的产物会被打包闸门拒绝 = 失败安全）。

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../构建/构建指纹算法.rs"));

fn main() {
    let 源码根 = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let 指纹 = if 源码根.is_dir() {
        计算目录指纹(&源码根)
    } else {
        eprintln!("[警告] 基础功能/src 不存在，构建标识降级为「未知」（该产物会被打包闸门拒绝）");
        "未知".to_string()
    };
    println!("cargo:rustc-env=基础指纹={}", 指纹);

    // 改任一源文件必须重算并重编（否则内嵌标识会停留在旧值）
    println!("cargo:rerun-if-changed={}", 源码根.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../构建/构建指纹算法.rs")
    );
}
