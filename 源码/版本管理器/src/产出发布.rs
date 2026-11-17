// 产出发布 — 将 Cargo 构建产物打包为智语言开发套件发布文件
// 用法: cargo run --bin 产出发布 [开发套件版本号，省略则读 Cargo.toml 的 version]
// 输出（发布目录：版本目录隔离 + 三分类子目录）:
//   发布/版本<号>/安装包/智语言开发套件<版本>(.exe)      (自解压安装程序，规范 6.1.6 命名：不带平台后缀)
//   发布/版本<号>/便携版/智语言开发套件<版本>-<平台>.zip (完整套件压缩包，解压即用)
//   发布/版本<号>/便携版/智语言开发套件<版本>-<平台>.zip.sha256 (校验和，跟包走)
//   发布/版本<号>/校验/SHA256SUMS                       (汇总安装+便携全部文件)
// 安装程序 = 安装程序本体 exe + 追加的「同批 ZIP 载荷」，双击后交互选择安装目录，
// 安装结果与「ZIP 解压」逐目录一致（版本管理器/ + 版本<号>/…）。
// ZIP 内布局（规范 6.1.1 安装后目录；**顶层统一为 `智语言/`**，解压即得同名目录）:
//   智语言/版本管理器/智语言(.exe)  ← 版本管理器
//   智语言/版本<版本号>/            ← 该版本工具包（执行文件/标准库/工具链/许可证.文档）
//   智语言/版本<版本号>/执行文件/    ← 智语言(.exe) + 智语言-编辑器服务(.exe)
//                                    + base.lib/libbase.a + .base_build_fingerprint（编译模式必需）
//   说明：规范 6.1.1 的 `版本<版本号>/文档/`（语言参考/标准库说明/工具说明）
//        **暂不随包**（2026-11-16：三份文档内容尚不完善），恢复方式见下方「==== 文档 ====」注释。
//   顶层目录名单一真相 = launcher::包内顶层目录（安装器/安装程序 解包时按它剥离）。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = env::args().collect();
    // 开发套件版本号：命令行参数优先，否则读 Cargo.toml 的 version（与 命令行入口.rs 同机制）
    let 开发套件版本号 = if args.len() > 1 {
        args[1].clone()
    } else {
        env!("CARGO_PKG_VERSION").to_string()
    };
    // 语义化版本校验（规范：x.y.z 三段式）
    if launcher::开发套件版本解析::解析版本(&开发套件版本号).is_none() {
        eprintln!("[错误] 非法版本号: {}（须为 x.y.z 三段式）", 开发套件版本号);
        return;
    }

    let 平台 = launcher::开发套件版本解析::平台标识();
    let 包名 = format!("智语言开发套件{}-{}", 开发套件版本号, 平台);

    // 路径
    let 仓库根 = 探测仓库根();
    // ⚠ 2026-11-16 发布安全修复：**发布件必须是 release 产物**。
    //   原实现从 `产出/debug` 取 exe ⇒ 发布出去的二进制是 debug 构建：
    //     ① 泄漏本机构建目录（实测二进制内可读到 `…\智语言\产出\debug`）与调试信息；
    //     ② 把 debug 断言（整数溢出 panic 等）带进用户手上，行为与 release 不一致。
    //   现在只认 `产出/release`，缺失时明确报错（不静默退化为 debug）。
    let 产出源 = 仓库根.join("产出").join("release");
    let 发布根 = 仓库根.join("发布");
    let 版本发布根 = 发布根.join(format!("版本{}", 开发套件版本号));
    let 安装目录 = 版本发布根.join("安装包");
    let 便携目录 = 版本发布根.join("便携版");
    let 校验目录 = 版本发布根.join("校验");

    if !产出源.exists() {
        eprintln!("[错误] 没有 release 产出目录：{}", 产出源.display());
        eprintln!("       发布必须使用 release 产物（debug 会泄漏构建路径且行为不同）。");
        eprintln!("       请先执行（仓库根，离线加 --offline）：");
        eprintln!("         构建发布：python 测试/3测试脚本/构建发布.py   （release + 路径脱敏）");
        eprintln!("       或手工：");
        eprintln!("         cargo build --release -p basic_functions --target-dir 产出");
        eprintln!("         cargo build --release --bin 智语言 --bin 智语言开发套件 --bin 产出发布 \\");
        eprintln!("                     --bin 智语言安装程序 --bin 智语言-编辑器服务 --target-dir 产出");
        std::process::exit(2);
    }

    eprintln!("=== 打包: {} → 发布/版本{}/ ===", 包名, 开发套件版本号);

    // 清理旧发布：只清当前版本目录，其它版本保留（换版本自动新建，旧版本原样不动）
    let _ = fs::remove_dir_all(&版本发布根);
    fs::create_dir_all(&安装目录).expect("创建安装包目录失败");
    fs::create_dir_all(&便携目录).expect("创建便携版目录失败");
    fs::create_dir_all(&校验目录).expect("创建校验目录失败");

    // 打包工作目录建在发布根外（日志/脚本/临时/），发布根始终保持干净
    let 工作 = 仓库根.join("日志").join("脚本").join("临时").join(&包名);
    let _ = fs::remove_dir_all(&工作);
    let 版本目录 = 工作.join(format!("版本{}", 开发套件版本号));
    // ⚠ 不再预建 `版本<号>/文档`、`版本<号>/工具`：两者从未写入任何内容，
    //   而 ZIP 只存文件、空目录解压后不存在 ⇒ 便携版解压结果里本来就没有它们。
    //   安装程序把「安装结果 = 便携版解压结果」当作契约，故打包侧也去掉，避免两处不一致。
    let 目录列表 = [
        "版本管理器".to_string(),
        format!("版本{}/执行文件", 开发套件版本号),
        format!("版本{}/标准库", 开发套件版本号),
        format!("版本{}/工具链", 开发套件版本号),
    ];
    for d in &目录列表 {
        fs::create_dir_all(工作.join(d)).expect("创建工作目录失败");
    }

    // ==== 版本管理器（引导程序） ====
    let 引导名 = base::平台::可执行文件名("智语言");
    let 工具包名 = base::平台::可执行文件名("智语言开发套件");
    let 引导源 = 产出源.join(&引导名);
    if 引导源.exists() {
        let 引导目标 = 工作.join("版本管理器").join(&引导名);
        fs::copy(&引导源, &引导目标).ok();
        let _ = base::平台::使可执行(&引导目标);
        eprintln!("  [OK] 版本管理器/{}", 引导名);
        // 版本.文档：安装痕迹（与 exe 同源同批，供安装器比较版本决定覆盖/跳过）
        fs::write(
            工作.join("版本管理器/版本.文档"),
            format!("版本管理器版本 = {}\n", launcher::版本管理器版本号),
        ).ok();
        eprintln!("  [OK] 版本管理器/版本.文档 = {}", launcher::版本管理器版本号);
    } else {
        eprintln!("  [跳过] {} 不存在", 引导名);
    }

    // 运行时静态库与指纹的位置（安装包/ 与 版本目录/执行文件/ 两处都要放，故提前定义）
    let 库名 = format!("{}base{}", base::平台::库前缀(), base::平台::库后缀());
    let 基础产出 = 仓库根.join("产出").join("release");
    let 库源 = 基础产出.join(&库名);

    // ==== 工具包本体（执行文件） ====
    let 工具包源 = 产出源.join(&工具包名);
    if 工具包源.exists() {
        // 放置时统一改名为引导名（运行入口 = 智语言(.exe)），与安装布局一致
        let 本体目标 = 版本目录.join("执行文件").join(&引导名);
        fs::copy(&工具包源, &本体目标).ok();
        let _ = base::平台::使可执行(&本体目标);
        eprintln!("  [OK] 版本{}/执行文件/{}", 开发套件版本号, 引导名);

        // ⚠ 2026-11-03 重构：此前的「安装包/」只是把工具包本体 exe 拷贝一份
        //   （外加 base.lib 与使用说明），**不是一个安装器**：用户双击后得到的仍是
        //   "免安装单文件套装"，与规范 6.1.5「首次安装提示用户选择安装目录」和
        //   6.1.6「安装程序」的定义不符，也与用户对安装包的直觉不符。
        //   现在「安装包/」只产出一个**自解压安装程序**（本体 exe + 追加的便携版 ZIP 载荷），
        //   生成逻辑在本文件末尾的 `制造安装程序`（ZIP 构造完成之后才能追加）。
    } else {
        eprintln!("  [跳过] {} 不存在", 工具包名);
    }

    // ==== 编辑器语言服务（LSP 服务器）====
    // ★ 2026-10-08 补：**必须与工具包本体同置 `执行文件/`**。
    //   编辑器插件按 `<安装目录>/版本X.Y.Z/执行文件/智语言-编辑器服务(.exe)` 自动探测
    //   （见 `智语言编辑器插件/源码/配置/设置.ts::探测语言服务器`）。
    //   此前本打包器只放 引导程序 + 工具包本体两个 exe ⇒ 用户装完套件**根本没有 LSP 服务器**，
    //   而插件 `智语言.启用语言服务` 默认**开启** ⇒ 每次启动都会尝试拉起一个不存在的命令
    //   （跳转/大纲/诊断全部静默不可用，且报错信息指向路径而非"未随套件分发"）。
    //   安装器是**整目录复制**（`开发套件安装器::安装` 的 `复制目录`），故只需在此放入即可。
    let 编辑器服务名 = base::平台::可执行文件名("智语言-编辑器服务");
    let 编辑器服务源 = 产出源.join(&编辑器服务名);
    if 编辑器服务源.exists() {
        let 目标 = 版本目录.join("执行文件").join(&编辑器服务名);
        fs::copy(&编辑器服务源, &目标).ok();
        let _ = base::平台::使可执行(&目标);
        eprintln!("  [OK] 版本{}/执行文件/{}", 开发套件版本号, 编辑器服务名);
    } else {
        eprintln!("  [跳过] {} 不存在（编辑器插件将无跳转/大纲/诊断）", 编辑器服务名);
    }

    // ==== 运行时静态库 base（编译模式必需） ====
    // ★ 2026-11-03 补：此前打包清单**漏了 base.lib** ⇒ 便携版在**没有 Rust 源码/Cargo.toml 的机器**上
    //   编译模式必然失败。实测（解压便携版后 `构建运行 --重新编译`）：
    //     「构建 基础功能 staticlib 失败: manifest path …\Cargo.toml does not exist」
    //     随后 rust-lld 报一堆 `智语言_设置调用深度上限 / 智语言_堆操作 / 智语言启动` undefined。
    //   落点 = `执行文件/`：链接器 `自动查找运行时库` 的第 2 条搜索路径就是 **current_exe 同目录**
    //   （工具包本体 exe 正在该目录，故 A 命中、不会再去 cargo 重建）。
    //   必须同时带 `.base_build_fingerprint`：`确保运行时库已构建` 会比对它的内容与本体内置
    //   `基础指纹`，缺失/不一致就判定过期并尝试 cargo 重建（便携机无 cargo ⇒ 失败）。
    if 库源.exists() {
        // ⚠ 2026-11-03 一致性闸门（重做：判据全部来自产物自身）
        //   旧实现读 产出/release/.base_build_fingerprint 这个**外部标记文件**，而它只由
        //   `链接器::构建运行时静态库` 在重建成功后写 ⇒ 任何人直接 `cargo build -p basic_functions`
        //   都会得到「新库 + 旧标记」，闸门因此**假失败**（实测踩过）。
        //   现在身份随产物走：base.lib 与 工具 exe 各自内含 `ZHI-BASE-FP:<hex>`
        //   （见 基础功能/src/运行时/构建标识.rs 与 版本管理器/src/构建签名.rs），
        //   闸门只做「两个待发布产物对读」—— 既不依赖标记文件，也不依赖打包器自身的构建时间
        //   （打包器只重建了部分 bin 时它可能陈旧，若拿它的常量当判据会误拒正确的产物）。
        let 库指纹 = match base::构建标识::读取内嵌标识(&库源) {
            Ok(值) => 值,
            Err(原因) => {
                eprintln!("[错误] 无法确认 产出/release/{} 的构建身份：{}", 库名, 原因);
                eprintln!("       重建方法（在仓库根执行，离线环境加 --offline）：");
                eprintln!("         cargo build --release -p basic_functions --target-dir 产出");
                std::process::exit(2);
            }
        };

        // 与工具包本体对读：两者必须同源同批。否则会发「新 exe + 旧库」的包，
        // 而便携机（无源码/Cargo.toml）无法自救 ⇒ 每次编译一条注定失败的警告。
        if 工具包源.exists() {
            match base::构建标识::读取内嵌标识(&工具包源) {
                Ok(工具指纹) if 工具指纹 == 库指纹 => {}
                Ok(工具指纹) => {
                    eprintln!("[错误] 工具包本体与运行时库不同源，拒绝打包。");
                    eprintln!("       工具包 {}：{}", 工具包名, 工具指纹);
                    eprintln!("       运行时库 {}：{}", 库名, 库指纹);
                    eprintln!("       常见原因：只重建了一边（例如只跑了单条 cargo build --bin …）。");
                    eprintln!("       修复（仓库根，离线加 --offline）：两边都重建后再打包：");
                    eprintln!("         cargo build --release -p basic_functions --target-dir 产出");
                    eprintln!("         cargo build --bin 智语言开发套件");
                    std::process::exit(2);
                }
                Err(原因) => {
                    eprintln!("[错误] 无法确认工具包本体的构建身份：{}", 原因);
                    eprintln!("       请整体重建：cargo build（离线加 --offline）");
                    std::process::exit(2);
                }
            }
        } else {
            eprintln!("  [警告] {} 不存在，跳过与运行时库的身份对读", 工具包名);
        }

        // 打包器自身若陈旧，其内置指纹会与产物不同 —— 仅提示，**不作为判据**（判据已由产物对读给出）。
        let 打包器内置指纹 = launcher::构建签名::基础指纹();
        if 打包器内置指纹 != 库指纹 {
            eprintln!(
                "  [提示] 打包器内置指纹 {} 与产物 {} 不同（打包器可能陈旧，已以产物为准）",
                打包器内置指纹, 库指纹
            );
        }

        fs::copy(&库源, 版本目录.join("执行文件").join(&库名)).ok();
        eprintln!("  [OK] 版本{}/执行文件/{}", 开发套件版本号, 库名);
        // 随包指纹 = 从产物读出的**真实身份**（既不是复制标记文件，也不是打包器常量）。
        fs::write(
            版本目录.join("执行文件").join(".base_build_fingerprint"),
            &库指纹,
        )
        .ok();
        eprintln!(
            "  [OK] 版本{}/执行文件/.base_build_fingerprint = {}",
            开发套件版本号, 库指纹
        );
    } else {
        eprintln!(
            "  [跳过] 产出/release/{} 不存在（编译模式在目标机将尝试用 cargo 重建）",
            库名
        );
    }

    // ==== 标准库（仅 .智 源码一份，内建 native 后端运行时零消费，文件作 API 门面参考）====
    let 标准库源 = 仓库根.join("源码").join("标准库");
    if 标准库源.exists() {
        let count = 复制标准库(&标准库源, &版本目录.join("标准库"));
        eprintln!("  [OK] 版本{}/标准库/ ({} 文件)", 开发套件版本号, count);
    }

    // ==== 文档（规范 6.1.1 的 版本<号>/文档/ 三件套）====
    // ★ 2026-11-16 用户决定：**暂不随包**。理由=三份文档目前内容还不完善
    //   （语言参考=规范全文、标准库说明=门面汇总、工具说明=使用手册），
    //   先不把不完善的内容发给用户，避免用户按残缺文档使用。
    //   ⇒ 打包器**不创建** `版本<号>/文档/`，`发布/` 内也不出现这三个文件。
    //
    //   ⚠ 待文档完善后恢复的方法（代码仍保留单一真相，无需重新设计）：
    //     ① 在本处调用 `生成随包文档(&仓库根, &版本目录.join("文档"))`；
    //     ② 重新加回 `随包文档测试::三件套齐全且非空`（见 git 历史/本文件的旧版本）；
    //     ③ 手册与《开发套件构建发布指南》补回 `文档/` 目录说明。
    //   注意：门面注释与编译器报错目前仍写「见 文档/标准库说明.文档」「《语言参考》X.Y 节」，
    //        这些引用在文档随包后即可落地；文档未随包的版本里用户查不到，属已知取舍。

    // ==== 许可证 ====
    let 许可 = format!(
        "智语言开发套件 {}\n许可证: MIT License\nCopyright (c) 2026 智语言项目\n",
        开发套件版本号
    );
    fs::write(版本目录.join("许可证.文档"), &许可).ok();
    eprintln!("  [OK] 版本{}/许可证.文档", 开发套件版本号);

    // ==== 工具链（clang + llvm-ar + 内置头文件，中文路径友好的外部 C 编译，2026-08-05）====
    // 来源：文件/外部语言编译器/精简编译器/（仓库内裁剪后的 LLVM 子集，约 136MB）
    // 布局：版本<号>/工具链/（与 执行文件/ 同级；运行时经 exe 上级/工具链/clang.exe 查找）
    let 工具链源 = 仓库根
        .join("文件")
        .join("外部语言编译器")
        .join("精简编译器");
    if 工具链源.exists() {
        let 工具链目标 = 版本目录.join("工具链");
        let count = 复制整目录(&工具链源, &工具链目标);
        eprintln!("  [OK] 版本{}/工具链/ ({} 文件)", 开发套件版本号, count);
    } else {
        eprintln!("  [跳过] 未找到 文件/外部语言编译器/精简编译器/（外部 C 语言编译将不可用）");
    }

    // ==== 构造 ZIP → 便携版/ ====
    let zip文件 = 便携目录.join(format!("{}.zip", 包名));
    let 文件列表 = 收集文件列表(&工作);
    let 文件切片: Vec<(&str, &[u8])> = 文件列表.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    let zip数据 = libmgr::存储打包::构造存储zip(&文件切片);
    fs::write(&zip文件, &zip数据).expect("写入ZIP失败");
    let zip大小 = fs::metadata(&zip文件).unwrap().len();
    eprintln!("  [OK] 便携版/{}.zip ({:.1}MB)", 包名, zip大小 as f64 / 1048576.0);

    // ==== SHA256（校验跟包走：便携版/ 内，与 zip 配对） ====
    let sha = libmgr::校验::计算sha256(&zip数据);
    fs::write(format!("{}.sha256", zip文件.display()), &sha).ok();
    eprintln!("  [OK] 便携版/{}.zip.sha256 = {}...", 包名, &sha[..16]);

    // ==== 安装程序（自解压单文件，规范 6.1.5/6.1.6）====
    制造安装程序(&产出源, &安装目录, &开发套件版本号, &zip数据);

    // ==== 汇总校验 校验/SHA256SUMS（安装+便携全部文件，业界格式 哈希  路径） ====
    let mut 汇总 = String::new();
    for (目录, 前缀) in [(&安装目录, "安装包"), (&便携目录, "便携版")] {
        if let Ok(条目) = fs::read_dir(目录) {
            for e in 条目.flatten() {
                let 路径 = e.path();
                if 路径.is_file() && 路径.extension().and_then(|x| x.to_str()) != Some("sha256") {
                    if let Ok(数据) = fs::read(&路径) {
                        let s = libmgr::校验::计算sha256(&数据);
                        let 相对 = format!("{}/{}", 前缀, e.file_name().to_string_lossy());
                        汇总.push_str(&format!("{}  {}\n", s, 相对));
                    }
                }
            }
        }
    }
    let 条目数 = 汇总.lines().count();
    fs::write(校验目录.join("SHA256SUMS"), 汇总).ok();
    eprintln!("  [OK] 校验/SHA256SUMS ({} 条目)", 条目数);

    // ==== 清理临时工作目录（发布根只保留三分类目录） ====
    let _ = fs::remove_dir_all(&工作);

    // ==== 报告 ====
    eprintln!();
    eprintln!("=== 输出 ===");
    打印树(&版本发布根, 1);
    eprintln!();
    eprintln!("完成: {}", 版本发布根.display());
}

// ============ 辅助 ============

/// 生成「自解压安装程序」：安装程序本体 exe + 追加的便携版 ZIP 载荷 + 24 字节尾记录。
///
/// 文件布局（**与 `源码/版本管理器/src/安装程序.rs::读取内嵌载荷` 成对**，改一处必须改另一处）：
///   [安装程序 PE 映像][ZIP 载荷][偏移 u64 小端][长度 u64 小端]["ZHILANGI"]
///
/// 为什么用「追加到 exe 末尾」而不是 `include_bytes!`：
///   ZIP 载荷约 195MB，`include_bytes!` 会在编译期生成同尺寸的常量数组（编译内存与时间爆炸，
///   且每次改 payload 都要重编整个 crate）。而 PE 映像最后一个节之后的字节不会被 Windows
///   加载器读取（自解压程序、安装包的通行做法），运行时按尾记录 seek 回来即可。
fn 制造安装程序(产出源: &Path, 安装目录: &Path, 开发套件版本号: &str, 载荷: &[u8]) {
    let 本体名 = base::平台::可执行文件名("智语言安装程序");
    let 源 = 产出源.join(&本体名);
    if !源.exists() {
        eprintln!("  [跳过] {} 不存在（安装包缺失，仅便携版可用）", 本体名);
        return;
    }
    let 目标名 = base::平台::可执行文件名(&format!("智语言开发套件{}", 开发套件版本号));
    let mut 数据 = match fs::read(&源) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  [跳过] 读取 {} 失败: {}", 本体名, e);
            return;
        }
    };
    let 偏移 = 数据.len() as u64;
    let 长度 = 载荷.len() as u64;
    数据.extend_from_slice(载荷);
    数据.extend_from_slice(&偏移.to_le_bytes());
    数据.extend_from_slice(&长度.to_le_bytes());
    数据.extend_from_slice(b"ZHILANGI");

    let 目标 = 安装目录.join(&目标名);
    match fs::write(&目标, &数据) {
        Ok(_) => {
            let _ = base::平台::使可执行(&目标);
            eprintln!(
                "  [OK] 安装包/{} ({:.1}MB，自解压：双击 → 选目录 → 安装)",
                目标名,
                数据.len() as f64 / 1048576.0
            );
        }
        Err(e) => eprintln!("  [跳过] 写入安装程序失败: {}", e),
    }
}

/// 自动检测项目根（从 Cargo 工作区 Cargo.toml 位置推算）
fn 探测仓库根() -> PathBuf {
    // 尝试从 CARGO_MANIFEST_DIR 推算（二进制在 版本管理器 crate 中）
    if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(&dir);
        // 版本管理器 crate 在 源码/版本管理器/，仓库根在上一级
        let 根 = p.parent().and_then(|p| p.parent());
        if let Some(r) = 根 {
            if r.join("Cargo.toml").exists() {
                return r.to_path_buf();
            }
        }
    }
    // 回退：当前目录
    env::current_dir().unwrap_or_default()
}

/// 递归收集目录下所有文件的 (ZIP路径, 字节内容)
///
/// ⚠ ZIP 内一律加一层顶层目录 `智语言/`（`launcher::包内顶层目录`）：
/// 用户解压便携版后得到 `智语言/版本管理器/…`，与文档里所有目录树的顶层一致。
/// 解包侧（开发套件安装器 / 安装程序）负责把这层剥掉，故两侧只共享同一个常量。
fn 收集文件列表(根: &Path) -> Vec<(String, Vec<u8>)> {
    let mut 结果 = Vec::new();
    递归收集(根, 根, &mut 结果);
    结果.sort_by(|a, b| a.0.cmp(&b.0));
    结果
        .into_iter()
        .map(|(名, 数据)| (format!("{}/{}", launcher::包内顶层目录, 名), 数据))
        .collect()
}

fn 递归收集(基: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    if let Ok(条目) = fs::read_dir(dir) {
        for e in 条目.flatten() {
            let 路径 = e.path();
            if 路径.is_dir() {
                递归收集(基, &路径, out);
            } else if let Ok(数据) = fs::read(&路径) {
                if let Ok(相对) = 路径.strip_prefix(基) {
                    let 名 = 相对.to_string_lossy().replace('\\', "/");
                    out.push((名, 数据));
                }
            }
        }
    }
}

/// 复制标准库 .智 文件到目标目录，返回文件数
fn 复制标准库(源: &Path, 目标: &Path) -> usize {
    let mut 计数 = 0;
    if !源.is_dir() { return 0; }
    if let Ok(条目) = fs::read_dir(源) {
        for 条目项 in 条目.flatten() {
            let 路径 = 条目项.path();
            if 路径.is_dir() {
                计数 += 复制标准库(&路径, &目标.join(条目项.file_name()));
            } else if 路径.extension().map_or(false, |x| x == "智") {
                fs::create_dir_all(目标).ok();
                fs::copy(&路径, 目标.join(条目项.file_name())).ok();
                计数 += 1;
            }
        }
    }
    计数
}

/// 整目录递归复制（含所有文件，不筛选扩展名），返回复制的文件数
fn 复制整目录(源: &Path, 目标: &Path) -> usize {
    let mut 计数 = 0;
    if !源.is_dir() { return 0; }
    fs::create_dir_all(目标).ok();
    if let Ok(条目) = fs::read_dir(源) {
        for 条目项 in 条目.flatten() {
            let 路径 = 条目项.path();
            let 目标路径 = 目标.join(条目项.file_name());
            if 路径.is_dir() {
                计数 += 复制整目录(&路径, &目标路径);
            } else {
                fs::copy(&路径, &目标路径).ok();
                计数 += 1;
            }
        }
    }
    计数
}

#[cfg(test)]
mod 门面引用测试 {
    use super::*;

    /// 契约：随包门面不得出现 Rust 源码路径（`标准库元数据.rs`，用户拿不到）
    /// 或规范里不存在的 `§行号`（规范正文只有章节标题 `### 6.5.3`）。
    ///
    /// 说明：`文档/标准库说明.文档` 目前**不随包**（2026-11-16 用户决定：文档尚不完善），
    ///   门面注释里的「见 文档/标准库说明.文档」属**前瞻引用**；该文档完善并随包后即落地。
    #[test]
    fn 门面不引用源码路径与失效编号() {
        let 标准库 = 探测仓库根().join("源码").join("标准库");
        let mut 检查数 = 0;
        for 项 in fs::read_dir(&标准库).expect("读不到 源码/标准库").flatten() {
            let 路径 = 项.path();
            if 路径.extension().map_or(true, |扩| 扩 != "智") {
                continue;
            }
            let 内容 = fs::read_to_string(&路径).unwrap_or_default();
            检查数 += 1;
            assert!(
                !内容.contains("标准库元数据.rs"),
                "{} 引用了 Rust 源码路径（用户拿不到），应指向 文档/标准库说明.文档",
                路径.display()
            );
            assert!(
                !内容.contains("§"),
                "{} 含 § 引用：规范正文只有章节标题、无 § 编号，应写「《语言参考》X.Y 节」",
                路径.display()
            );
        }
        assert!(检查数 > 0, "没有找到任何门面文件");
    }
}

/// 打印目录树（报告用）
fn 打印树(dir: &Path, 缩进: usize) {
    let 前缀 = "  ".repeat(缩进);
    if let Ok(条目) = fs::read_dir(dir) {
        let mut 收集: Vec<_> = 条目.flatten().collect();
        收集.sort_by_key(|e| e.file_name());
        for e in 收集 {
            let 名 = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() {
                eprintln!("{}{}/", 前缀, 名);
                打印树(&e.path(), 缩进 + 1);
            } else {
                let kb = e.metadata().map(|m| m.len() as f64 / 1024.0).unwrap_or(0.0);
                let mb = if kb > 1024.0 { format!("{:.1}MB", kb / 1024.0) } else { format!("{:.0}KB", kb) };
                eprintln!("{} {}  {}", 前缀, 名, mb);
            }
        }
    }
}
