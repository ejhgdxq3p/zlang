// 智语言版本管理器 - 统一入口、版本匹配、命令转发（规范 6.1.3/6.1.4）
// 项目规范：全中文命名（中文标识符必然触发 non_snake_case）——与 4 个 lib crate 一致豁免该 lint。
#![allow(non_snake_case)]
use std::env;
use std::process;

// 版本管理器版本号共享常量定义在 lib（库.rs）：launcher::版本管理器版本号

fn main() {
    let args: Vec<String> = env::args().collect();
    // G8：自动检查驱动（非阻塞、失败静默，绝不以 todo!() 卡住）
    let 配置 = pm::全局配置::读取配置();
    if 配置.更新.自动检查 {
        // 后台非阻塞检查更新（离线/无服务器时由镜像源内部明确报错，不阻塞主流程）
        std::thread::spawn(|| {
            let _ = launcher::开发套件镜像源::查询可安装版本(false);
        });
    }
    if args.len() < 2 { 打印帮助(None); return; }
    let (命令, 参数) = (&args[1], &args[2..]);
    if 是工具包命令(命令) { 执行工具包命令(命令, 参数); return; }
    let 匹配版 = 匹配工具包版本();
    转发(&匹配版, 命令, 参数);
}

// ======================== 版本匹配 ========================
fn 匹配工具包版本() -> String {
    let cwd = env::current_dir().unwrap_or_default();
    if let Some(根) = pm::依赖解析::检测项目根(&cwd) {
        if let Some(约束) = pm::依赖解析::解析开发套件版本约束(&根) {
            let 已装 = pm::目录::扫描已安装版本();
            if let Ok(最佳) = launcher::开发套件版本解析::选择最佳版本(&已装, &约束) {
                return 最佳;
            }
            eprint!("[智语言] 项目需要开发套件 {}，但未安装。是否安装？[Y/n] ", 约束);
            if let Ok(回答) = 读取一行() {
                if 回答.eq_ignore_ascii_case("y") || 回答.eq_ignore_ascii_case("yes") {
                    if let Ok(列表) = launcher::开发套件镜像源::查询可安装版本(false) {
                        if let Ok(最佳) = launcher::开发套件版本解析::选择最佳版本(&列表, &约束) {
                            if launcher::开发套件安装器::安装(&最佳, None, None).is_ok() {
                                return 最佳;
                            }
                        }
                    }
                    eprintln!("[智语言] 安装失败");
                }
            }
            process::exit(1);
        }
    }
    pm::全局配置::当前激活版本()
}

fn 读取一行() -> Result<String, ()> {
    let mut 输入 = String::new(); std::io::stdin().read_line(&mut 输入).map_err(|_| ())?; Ok(输入.trim().to_string())
}

// ======================== 转发 ========================
fn 转发(版本: &str, 命令: &str, 参数: &[String]) {
    let 本体 = pm::目录::版本可执行文件(版本);
    if !本体.exists() {
        eprintln!("[智语言] 错误：开发套件版本 {} 未安装", 版本);
        process::exit(1);
    }
    match process::Command::new(&本体).arg(命令).args(参数).env("智语言_已引导", "1").status() {
        Ok(s) => process::exit(s.code().unwrap_or(1)),
        Err(e) => { eprintln!("[智语言] 错误：{}", e); process::exit(1); }
    }
}

// ======================== 命令分发 ========================
fn 是工具包命令(命令: &str) -> bool {
    matches!(命令, "版本"|"安装"|"卸载"|"更新"|"已安装"|"可安装"|"信息"
        |"切换"|"回退"|"设置"|"恢复"|"帮助"|"诊断"|"自我更新"|"卸载全部")
}

fn 执行工具包命令(命令: &str, 参数: &[String]) {
    match 命令 {
        "版本" => println!("{}", 匹配工具包版本()),
        "已安装" => 执行已安装(),
        "可安装" => 执行可安装(参数),
        "安装" => 执行安装(参数),
        "卸载" => 执行卸载(参数),
        "更新" => 执行更新(参数),
        "切换" => 执行切换(参数),
        "回退" => 执行回退(),
        "信息" => 执行信息(参数),
        "设置" => 执行设置(参数),
        "恢复" => 执行恢复(参数),
        "诊断" => 执行诊断(),
        "自我更新" => 执行自我更新(),
        "卸载全部" => 执行卸载全部(),
        "帮助" => 打印帮助(参数.first().map(|s| s.as_str())),
        _ => {}
    }
}

// ======================== 24条命令实现 ========================

fn 执行已安装() {
    let 激活 = pm::全局配置::当前激活版本();
    let 列表 = pm::目录::扫描已安装版本();
    if 列表.is_empty() { eprintln!("[智语言] 没有已安装的开发套件"); return; }
    eprintln!("[智语言] 已安装的开发套件：");
    for v in &列表 { if v == &激活 { eprintln!("* {} （当前激活）", v); } else { eprintln!("  {}", v); } }
}

fn 执行可安装(参数: &[String]) {
    let 测试 = 参数.iter().any(|p| p == "--测试版");
    match launcher::开发套件镜像源::查询可安装版本(测试) {
        Ok(列表) => { for v in 列表 { eprintln!("  {}", v); } }
        Err(e) => eprintln!("[智语言] 错误：{}", e),
    }
}

fn 执行安装(参数: &[String]) {
    let 仅下载 = 参数.iter().any(|p| p == "--仅下载");
    let 测试版 = 参数.iter().any(|p| p == "--测试版");
    let mut 版本号 = None; let mut 安装路径 = None; let mut 本地包 = None;
    for p in 参数 {
        if p.starts_with("--") { continue; }
        if p.ends_with(".zip") || p.ends_with(".tar.gz") { 本地包 = Some(p.as_str()); }
        else if p.contains('/') || p.contains('\\') || (p.len() >= 2 && &p[1..2] == ":") { 安装路径 = Some(p.as_str()); }
        else { 版本号 = Some(p.as_str()); }
    }
    // ⚠ 2026-10-03 修复：`--测试版` 原先两个分支都取 `"latest"`（`测试版.then_some("latest").unwrap_or("latest")`）
    //   ⇒ 该开关**形同虚设**，`智语言 安装 --测试版` 装到的是稳定版 latest，与帮助文本承诺不符。
    //   现统一在此解析：显式版本号优先；否则 `--测试版` ⇒ 最高测试版；再否则 ⇒ latest（最高稳定版）。
    let 目标: String = match 版本号 {
        Some(v) => v.to_string(),
        None if 测试版 => match launcher::开发套件镜像源::最新测试版() {
            Ok(v) => v,
            Err(e) => { eprintln!("[智语言] 错误：{}", e); return; }
        },
        None => "latest".to_string(),
    };
    if 仅下载 {
        // 自定义路径优先：安装路径既可用于安装位置，也可用于仅下载目标目录
        let 目标目录 = 安装路径
            .map(|p| std::path::Path::new(p).to_path_buf())
            .unwrap_or_else(|| pm::目录::开发套件下载目录());
        match launcher::开发套件镜像源::下载工具包(&目标, &目标目录) {
            Ok((z, _)) => eprintln!("[智语言] 下载完成：{}", z.display()),
            Err(e) => eprintln!("[智语言] 错误：{}", e),
        }
        return;
    }
    match launcher::开发套件安装器::安装(&目标, 本地包.map(|p| std::path::Path::new(p)), 安装路径.map(|p| std::path::Path::new(p))) {
        Ok(_) => {}
        Err(e) => eprintln!("[智语言] 错误：{}", e),
    }
}

fn 执行卸载(参数: &[String]) {
    let Some(v) = 参数.first() else { eprintln!("[智语言] 错误：请指定版本号"); return; };
    if let Err(e) = launcher::开发套件安装器::卸载(v) { eprintln!("[智语言] 错误：{}", e); }
}

fn 执行更新(参数: &[String]) {
    let 目标 = if let Some(v) = 参数.first() { v.clone() } else { "latest".to_string() };
    // 先把 latest/最新 解析成真实版本号，否则会打印"已更新至 latest"这种无意义提示
    let 实际版本 = match launcher::开发套件镜像源::解析版本别名(&目标) {
        Ok(v) => v,
        Err(e) => { eprintln!("[智语言] 错误：{}", e); return; }
    };
    // 已是最新不是错误：属正常状态，给出提示并正常返回，避免误报失败
    if pm::目录::版本目录(&实际版本).join("执行文件").exists() {
        eprintln!("[智语言] 已是最新版本 {}，无需更新", 实际版本);
        return;
    }
    if let Err(e) = launcher::开发套件安装器::安装(&实际版本, None, None) {
        eprintln!("[智语言] 错误：{}", e); return;
    }
    eprintln!("[智语言] 已更新至 {}", 实际版本);
}

/// ⚡双模式⚡ C3 自我更新线程化：在后台线程执行最新版安装（覆盖更新开发套件本体），
/// 不阻塞版本管理器主流程；离线/无服务器时由 开发套件安装器 内部明确报错，绝不以 todo!() 静默。
fn 执行自我更新() {
    eprintln!("[智语言] 正在后台自我更新...");
    let 句柄 = std::thread::spawn(|| {
        // 已是最新属正常状态，不按失败上报
        let 实际版本 = match launcher::开发套件镜像源::解析版本别名("latest") {
            Ok(v) => v,
            Err(e) => { eprintln!("[智语言] 自我更新失败：{}", e); return; }
        };
        if pm::目录::版本目录(&实际版本).join("执行文件").exists() {
            eprintln!("[智语言] 已是最新版本 {}，无需自我更新", 实际版本);
            return;
        }
        match launcher::开发套件安装器::安装(&实际版本, None, None) {
            Ok(_) => eprintln!("[智语言] 自我更新完成（{}），请重启版本管理器以生效", 实际版本),
            Err(e) => eprintln!("[智语言] 自我更新失败：{}", e),
        }
    });
    // 必须 join：主线程若先退出，后台线程的结果提示会被丢弃，用户看不到任何结论
    let _ = 句柄.join();
}

fn 执行切换(参数: &[String]) {
    let Some(v) = 参数.first() else { eprintln!("[智语言] 错误：请指定版本号"); return; };
    if !pm::目录::版本目录(v).exists() { eprintln!("[智语言] 错误：版本 {} 未安装", v); return; }
    let mut 配置 = pm::全局配置::读取配置();
    match 配置.切换版本(v) { Ok(_) => eprintln!("[智语言] 已切换至开发套件 {}", v), Err(e) => eprintln!("[智语言] 错误：{}", e) }
}

fn 执行回退() {
    let mut 配置 = pm::全局配置::读取配置();
    // ★ 2026-10-03 修复：回退前必须校验目标版本**已安装**（与 `执行切换` 同口径）。
    //   ⚠ 原实现直接 `回退版本()` 做交换并落盘：全局.配置 的 开发套件版本 默认是 `0.0.1`，
    //   于是**首次 `安装 x`** 会把 回退记录 置成 `0.0.1`（该版本从未安装），此后执行 `回退`
    //   就把激活版本指向**不存在的目录** ✗ —— 实测输出「已回退至开发套件 0.0.1」而 安装根 只有 版本1.0.0，
    //   随后任何被转发的业务命令（运行/构建/库…）都会报「开发套件版本 0.0.1 未安装」，用户不明所以。
    //   ⇒ 先取 回退记录 校验存在性，不通过就**不改配置**并给出可操作提示。
    if 配置.开发套件.回退记录.is_empty() {
        eprintln!("[智语言] 错误：没有可回退的版本记录");
        return;
    }
    let 目标 = 配置.开发套件.回退记录.clone();
    if !pm::目录::版本目录(&目标).exists() {
        eprintln!(
            "[智语言] 错误：回退目标版本 {} 未安装，无法回退（执行「智语言 已安装」查看已装版本）",
            目标
        );
        return;
    }
    match 配置.回退版本() { Ok(v) => eprintln!("[智语言] 已回退至开发套件 {}", v), Err(e) => eprintln!("[智语言] 错误：{}", e) }
}

fn 执行信息(参数: &[String]) {
    let 默认版本 = pm::全局配置::当前激活版本();
    let 版本 = 参数.first().map(|s| s.as_str()).unwrap_or(&默认版本);
    let 根 = pm::目录::版本目录(版本);
    if !根.exists() {
        // 本地未安装：回退查询镜像源，展示远程版本详情（离线/无此版本时给出明确原因）
        eprintln!("[智语言] 版本 {} 未安装，改为查询镜像源…", 版本);
        match launcher::开发套件镜像源::查询版本详情(版本) {
            Ok(详情) => 打印远程版本详情(&详情),
            Err(错误) => eprintln!("[智语言] 查询镜像源失败：{}", 错误),
        }
        return;
    }
    eprintln!("[智语言] 智语言开发套件 {}", 版本);
    eprintln!("[智语言] 位置：{}", 根.display());
    let 执行目录 = 根.join("执行文件");
    if let Ok(条目) = std::fs::read_dir(&执行目录) {
        let 工具: Vec<String> = 条目.flatten().filter_map(|e| e.file_name().to_str().map(|s| s.to_string())).collect();
        if !工具.is_empty() { eprintln!("[智语言] 工具：{}", 工具.join(", ")); }
    }
    let 许可 = 根.join("许可证.文档");
    if 许可.exists() { eprintln!("[智语言] 许可证：{}", std::fs::read_to_string(&许可).unwrap_or_default().trim()); }
}

/// 打印镜像源返回的版本详情（供 信息 命令在本地未安装时使用）
fn 打印远程版本详情(详情: &launcher::开发套件镜像源::版本详情) {
    eprintln!("[智语言] 智语言开发套件 {}（镜像源）", 详情.版本号);
    if !详情.发布时间.is_empty() {
        eprintln!("[智语言] 发布日期：{}", 详情.发布时间);
    }
    if !详情.说明.is_empty() {
        eprintln!("[智语言] 更新内容：{}", 详情.说明);
    }
    if 详情.平台列表.is_empty() {
        eprintln!("[智语言] 可用平台：（无）");
    } else {
        eprintln!("[智语言] 可用平台：{}", 详情.平台列表.join(", "));
    }
    eprintln!("[智语言] 安装命令：智语言 安装 {}", 详情.版本号);
}

fn 执行设置(参数: &[String]) {
    // 设置 <键> [值]：带值=设置，无值=查看当前生效值
    let 可设键 = "开发套件镜像源 / 库镜像源 / 代理 / 离线模式 / 并发下载数 / 超时 / 重试次数 / 自动检查 / 检查频率 / 体验改进计划";
    if 参数.is_empty() {
        eprintln!("[智语言] 用法：智语言 设置 <键> [值]");
        eprintln!("[智语言] 可设键：{}", 可设键);
        return;
    }
    let 键 = 参数[0].as_str();
    let mut 配置 = pm::全局配置::读取配置();
    if 参数.len() < 2 {
        // 无值：查看当前生效值
        match 键 {
            "开发套件镜像源" => eprintln!("[智语言] 开发套件镜像源：{}（{}）", 配置.开发套件镜像源地址(), if 配置.开发套件镜像源是用户设置() { "用户" } else { "官方" }),
            "库镜像源" => eprintln!("[智语言] 库镜像源：{}（{}）", 配置.库镜像源地址(), if 配置.库镜像源是用户设置() { "用户" } else { "官方" }),
            "代理" => eprintln!("[智语言] 代理：{}", if 配置.网络.代理.is_empty() { "（未设置）".to_string() } else { 配置.网络.代理.clone() }),
            "离线模式" => eprintln!("[智语言] 离线模式：{}", if 配置.网络.离线模式 { "真" } else { "假" }),
            "并发下载数" => eprintln!("[智语言] 并发下载数：{}", 配置.网络.并发下载数),
            "超时" => eprintln!("[智语言] 超时：{}", 配置.网络.超时),
            "重试次数" => eprintln!("[智语言] 重试次数：{}", 配置.网络.重试次数),
            // ⚠ 2026-11-16 隐私修复：此前 `自动检查`/`体验改进计划` 只出现在 `诊断` 输出里，
            //   `设置` 无法查看也无法修改 ⇒ 用户看到一个默认开启的数据收集开关却关不掉。
            "自动检查" => eprintln!("[智语言] 自动检查：{}", if 配置.更新.自动检查 { "真" } else { "假" }),
            "检查频率" => eprintln!("[智语言] 检查频率：{}", 配置.更新.检查频率),
            "体验改进计划" => eprintln!(
                "[智语言] 体验改进计划：{}（当前版本不上报任何数据；此开关仅为预留）",
                if 配置.体验改进计划.启用 { "真" } else { "假" }
            ),
            _ => eprintln!("[智语言] 未知配置项：{}\n[智语言] 可设键：{}", 键, 可设键),
        }
        return;
    }
    let 值 = 参数[1].as_str();
    match 键 {
        "开发套件镜像源" => { 配置.镜像源.开发套件镜像源 = 值.to_string(); eprintln!("[智语言] 开发套件镜像源已设置：{}", 值); }
        "库镜像源" => { 配置.镜像源.库镜像源 = 值.to_string(); eprintln!("[智语言] 库镜像源已设置：{}", 值); }
        "代理" => { 配置.网络.代理 = 值.to_string(); eprintln!("[智语言] 代理已设置：{}", 值); }
        "离线模式" => {
            let v = match 值 { "真" => true, "假" => false, _ => { eprintln!("[智语言] 离线模式 仅接受 真/假，收到：{}", 值); return; } };
            配置.网络.离线模式 = v; eprintln!("[智语言] 离线模式已设置：{}", 值);
        }
        "并发下载数" => {
            match 值.parse::<u32>() {
                Ok(n) => { 配置.网络.并发下载数 = n; eprintln!("[智语言] 并发下载数已设置：{}", n); }
                Err(_) => { eprintln!("[智语言] 并发下载数 需为整数，收到：{}", 值); return; }
            }
        }
        "超时" => {
            match 值.parse::<u32>() {
                Ok(n) => { 配置.网络.超时 = n; eprintln!("[智语言] 超时已设置：{}", n); }
                Err(_) => { eprintln!("[智语言] 超时 需为整数，收到：{}", 值); return; }
            }
        }
        "重试次数" => {
            match 值.parse::<u32>() {
                Ok(n) => { 配置.网络.重试次数 = n; eprintln!("[智语言] 重试次数已设置：{}", n); }
                Err(_) => { eprintln!("[智语言] 重试次数 需为整数，收到：{}", 值); return; }
            }
        }
        "自动检查" => {
            let v = match 值 { "真" => true, "假" => false, _ => { eprintln!("[智语言] 自动检查 仅接受 真/假，收到：{}", 值); return; } };
            配置.更新.自动检查 = v; eprintln!("[智语言] 自动检查已设置：{}", 值);
        }
        "检查频率" => {
            match 值 {
                "每次" | "每天" | "每周" | "每月" => {
                    配置.更新.检查频率 = 值.to_string();
                    eprintln!("[智语言] 检查频率已设置：{}", 值);
                }
                _ => { eprintln!("[智语言] 检查频率 仅接受 每次/每天/每周/每月，收到：{}", 值); return; }
            }
        }
        "体验改进计划" => {
            let v = match 值 { "真" => true, "假" => false, _ => { eprintln!("[智语言] 体验改进计划 仅接受 真/假，收到：{}", 值); return; } };
            配置.体验改进计划.启用 = v;
            eprintln!("[智语言] 体验改进计划已设置：{}", 值);
            if v {
                eprintln!("[智语言] 提示：当前版本尚未实现任何数据上报，开启后也不会向任何服务器发送数据");
            }
        }
        _ => { eprintln!("[智语言] 未知配置项：{}\n[智语言] 可设键：{}", 键, 可设键); return; }
    }
    if let Err(e) = 配置.保存() { eprintln!("[智语言] 错误：{}", e); }
}

/// 恢复 <键>：将配置项重置为默认值（官方源/出厂值）
fn 执行恢复(参数: &[String]) {
    let 可恢复键 = "开发套件镜像源 / 库镜像源 / 代理 / 离线模式 / 并发下载数 / 超时 / 重试次数 / 自动检查 / 检查频率 / 体验改进计划";
    if 参数.is_empty() {
        eprintln!("[智语言] 用法：智语言 恢复 <键>");
        eprintln!("[智语言] 可恢复键：{}", 可恢复键);
        return;
    }
    let 键 = 参数[0].as_str();
    let mut 配置 = pm::全局配置::读取配置();
    let 默认 = pm::全局配置::全局配置::默认();
    match 键 {
        "开发套件镜像源" => { 配置.镜像源.开发套件镜像源 = 默认.镜像源.开发套件镜像源.clone(); eprintln!("[智语言] 开发套件镜像源已恢复默认（官方源）"); }
        "库镜像源" => { 配置.镜像源.库镜像源 = 默认.镜像源.库镜像源.clone(); eprintln!("[智语言] 库镜像源已恢复默认（官方源）"); }
        "代理" => { 配置.网络.代理 = 默认.网络.代理.clone(); eprintln!("[智语言] 代理已恢复默认（未设置）"); }
        "离线模式" => { 配置.网络.离线模式 = 默认.网络.离线模式; eprintln!("[智语言] 离线模式已恢复默认：{}", if 默认.网络.离线模式 { "真" } else { "假" }); }
        "并发下载数" => { 配置.网络.并发下载数 = 默认.网络.并发下载数; eprintln!("[智语言] 并发下载数已恢复默认：{}", 默认.网络.并发下载数); }
        "超时" => { 配置.网络.超时 = 默认.网络.超时; eprintln!("[智语言] 超时已恢复默认：{}", 默认.网络.超时); }
        "重试次数" => { 配置.网络.重试次数 = 默认.网络.重试次数; eprintln!("[智语言] 重试次数已恢复默认：{}", 默认.网络.重试次数); }
        "自动检查" => { 配置.更新.自动检查 = 默认.更新.自动检查; eprintln!("[智语言] 自动检查已恢复默认：{}", if 默认.更新.自动检查 { "真" } else { "假" }); }
        "检查频率" => { 配置.更新.检查频率 = 默认.更新.检查频率.clone(); eprintln!("[智语言] 检查频率已恢复默认：{}", 默认.更新.检查频率); }
        "体验改进计划" => {
            配置.体验改进计划.启用 = 默认.体验改进计划.启用;
            eprintln!("[智语言] 体验改进计划已恢复默认：{}", if 默认.体验改进计划.启用 { "真" } else { "假" });
        }
        _ => { eprintln!("[智语言] 未知配置项：{}\n[智语言] 可恢复键：{}", 键, 可恢复键); return; }
    }
    if let Err(e) = 配置.保存() { eprintln!("[智语言] 错误：{}", e); }
}

fn 执行诊断() {
    let 安装根 = pm::目录::安装路径();
    let 配置路径 = pm::目录::全局配置文件();
    let 配置 = pm::全局配置::读取配置();
    eprintln!("[智语言] === 环境诊断 ===");
    eprintln!("[智语言] 安装路径：{} ({})", 安装根.display(), if 安装根.exists() { "存在" } else { "不存在" });
    eprintln!("[智语言] 全局配置：{} ({})", 配置路径.display(), if 配置路径.exists() { "存在" } else { "不存在" });
    // 版本管理器版本：读 版本.文档 安装痕迹（程序身份在 exe 内常量，外部记录在文档）
    let 管理器版本 = std::fs::read_to_string(pm::目录::引导程序目录().join("版本.文档"))
        .ok()
        .and_then(|内容| {
            内容.lines().find_map(|行| {
                let 行 = 行.trim();
                行.strip_prefix("版本管理器版本")
                    .map(|后| 后.trim().trim_start_matches('=').trim().to_string())
            })
        })
        .unwrap_or_else(|| "未知（无版本.文档）".to_string());
    eprintln!("[智语言] 版本管理器版本：{}", 管理器版本);
    let 已装 = pm::目录::扫描已安装版本();
    let 列表为空 = 已装.is_empty();
    let 版本列表 = 已装.join(", ");
    eprintln!("[智语言] 已安装版本：{}", if 列表为空 { "无".to_string() } else { 版本列表 });
    eprintln!("[智语言] 默认版本：{}", 配置.开发套件.开发套件版本);
    eprintln!("[智语言] 全局目录：{}", 配置.全局路径配置.全局目录);
    // 诊断必须显示"实际生效"地址：客户端解析顺序为 环境变量 > 全局配置 > 官方源，
    // 若这里只读配置，环境变量覆盖时会误导用户（显示官方源、实际连的是别处）。
    let 套件源 = launcher::开发套件镜像源::镜像源地址();
    let 套件来源 = if std::env::var("智语言_开发套件镜像源_URL").map(|v| !v.trim().is_empty()).unwrap_or(false)
        || std::env::var("智语言_开发套件_URL").map(|v| !v.trim().is_empty()).unwrap_or(false) { "环境变量" }
        else if 配置.开发套件镜像源是用户设置() { "用户" } else { "官方" };
    eprintln!("[智语言] 开发套件镜像源：{}（{}）", 套件源, 套件来源);
    let 库源来源 = if std::env::var("智语言_库镜像源_URL").map(|v| !v.trim().is_empty()).unwrap_or(false) { "环境变量" }
        else if 配置.库镜像源是用户设置() { "用户" } else { "官方" };
    let 库源 = match std::env::var("智语言_库镜像源_URL") {
        Ok(v) if !v.trim().is_empty() => v.trim().trim_end_matches('/').to_string(),
        _ => 配置.库镜像源地址().trim_end_matches('/').to_string(),
    };
    eprintln!("[智语言] 库镜像源：{}（{}）", 库源, 库源来源);
    eprintln!("[智语言] 离线模式：{}", if 配置.网络.离线模式 { "真" } else { "假" });
    eprintln!("[智语言] 超时：{} 秒　重试次数：{}", 配置.网络.超时, 配置.网络.重试次数);
    eprintln!("[智语言] 体验改进计划：{}", if 配置.体验改进计划.启用 { "已启用" } else { "未启用" });
    eprintln!("[智语言] 自动检查：{}", if 配置.更新.自动检查 { "开启" } else { "关闭" });
    eprintln!("[智语言] 检查频率：{}", 配置.更新.检查频率);
}

fn 执行卸载全部() {
    eprintln!("[智语言] 警告：将删除所有开发套件版本及版本管理器！");
    eprint!("输入 'yes' 确认：");
    // 必须显式输入 yes 才继续（读不到输入/输入其它一律取消，避免管道 EOF 误删）
    match 读取一行() {
        Ok(s) if s == "yes" => {}
        _ => {
            eprintln!("[智语言] 已取消");
            return;
        }
    }
    let 根 = pm::目录::安装路径();
    if !根.exists() {
        eprintln!("[智语言] 安装目录不存在，无需卸载");
        return;
    }

    // ⚠ 版本管理器**自身就运行在这个目录里**，Windows 不允许删除正在执行的 exe：
    //   直接 remove_dir_all 会以 os error 5（拒绝访问）失败，只留下半截目录。
    //   正确姿势：先尽力清空（各版本目录/标准库/工具链…），再整根删除；
    //   整根还是删不掉，就把删除动作交给"本进程退出后执行"的外壳命令。
    if let Ok(条目们) = std::fs::read_dir(&根) {
        for 条目 in 条目们.flatten() {
            let 路径 = 条目.path();
            let _ = if 路径.is_dir() {
                std::fs::remove_dir_all(&路径)
            } else {
                std::fs::remove_file(&路径)
            };
        }
    }
    if std::fs::remove_dir_all(&根).is_ok() {
        eprintln!("[智语言] 已卸载全部");
        return;
    }
    match 安排延迟删除(&根) {
        Ok(()) => eprintln!(
            "[智语言] 已清除所有版本；版本管理器自身正在运行，将在本进程退出约 2 秒后自动删除：\n           {}",
            根.display()
        ),
        Err(e) => eprintln!(
            "[智语言] 已清除所有版本；但无法自动删除目录 {}（{}）。\n           请关闭所有命令行窗口后手动删除该目录。",
            根.display(),
            e
        ),
    }
}

/// 让操作系统在本进程退出后删除整个安装根（Windows 不能自删正在运行的 exe）。
/// 命令文本由 `base::平台::延迟删除目录命令` 给出，三平台行为一致。
fn 安排延迟删除(根: &std::path::Path) -> std::io::Result<()> {
    let (外壳程序, 外壳参数) = base::平台::外壳();
    let 命令 = base::平台::延迟删除目录命令(&根.to_string_lossy());
    std::process::Command::new(外壳程序)
        .arg(外壳参数)
        .arg(命令)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

fn 打印帮助(子命令: Option<&str>) {
    match 子命令 {
        Some("安装") => eprintln!("智语言 安装 [版本] [路径]\n智语言 安装 <文件路径>\n智语言 安装 --仅下载 <版本>\n智语言 安装 --测试版"),
        Some("卸载") => eprintln!("智语言 卸载 <版本>"),
        Some("更新") => eprintln!("智语言 更新 [版本]"),
        Some("切换") => eprintln!("智语言 切换 <版本>"),
        Some("设置") => eprintln!("智语言 设置 <键> [值]   设置或查看配置项\n智语言 恢复 <键>         恢复配置项到默认值"),
        _ => {
            eprintln!("智语言 版本管理器 v{}", launcher::版本管理器版本号);
            eprintln!();
            eprintln!("开发套件管理命令（任意位置）：");
            eprintln!("  版本          显示当前使用的开发套件版本");
            eprintln!("  安装 [版本]   安装开发套件");
            eprintln!("  卸载 <版本>   卸载指定版本");
            eprintln!("  更新 [版本]   更新开发套件");
            eprintln!("  已安装        列出已安装版本");
            eprintln!("  可安装        查看远程可安装版本");
            eprintln!("  信息 [版本]   查看版本详情");
            eprintln!("  切换 <版本>   切换全局默认版本");
            eprintln!("  回退          回退到上一个版本");
            eprintln!("  设置 <键>     设置/查看配置项（开发套件镜像源/库镜像源/代理/离线模式/并发下载数/超时/重试次数）");
            eprintln!("  恢复 <键>     恢复配置项到默认值");
            eprintln!("  诊断          环境检查");
            eprintln!("  自我更新      更新版本管理器");
            eprintln!("  卸载全部      卸载所有版本");
            eprintln!();
            eprintln!("业务命令（转发到匹配版本的开发套件执行）：");
            eprintln!("  运行   构建   构建运行   测试   创建   库   ...");
        }
    }
}
