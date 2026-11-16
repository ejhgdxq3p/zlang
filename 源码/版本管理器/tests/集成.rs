// 开发套件系统 客户端 ↔ 服务端 集成测试（env-gated：需设置 智语言_集成测试=1 且本机有 node）
// 运行：智语言_集成测试=1 cargo test -p launcher --test 集成 -- --nocapture
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

use launcher::开发套件镜像源;
use libmgr::镜像源客户端;
use libmgr::库安装器::安装器 as 库安装器类型;

struct 服务守卫 {
    子进程: Child,
}

impl Drop for 服务守卫 {
    fn drop(&mut self) {
        let _ = self.子进程.kill();
        let _ = self.子进程.wait();
    }
}

fn 临时目录() -> PathBuf {
    let 基 = std::env::temp_dir();
    let 纳秒 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let 名 = format!("zhilang_devkit_test_{}_{}", std::process::id(), 纳秒);
    let p = 基.join(名);
    fs::create_dir_all(&p).unwrap();
    p
}

fn 等待就绪(地址: &str, 子进程: &mut Child) {
    // 用底层 TCP 端口探测判断 node 是否已在监听（不依赖 HTTP 状态码，避免 ureq 把 404 当错误）
    let 主体 = 地址
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let 端口 = match 主体.split_once(':') {
        Some((_, p)) => p.parse::<u16>().unwrap_or(0),
        None => {
            eprintln!("[等待就绪] 无法解析地址 {} 的端口", 地址);
            0
        }
    };
    for _ in 0..50 {
        if std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], 端口)),
            Duration::from_secs(1),
        )
        .is_ok()
        {
            return;
        }
        if let Ok(Some(码)) = 子进程.try_wait() {
            let mut 错误输出 = String::new();
            if let Some(mut e) = 子进程.stderr.take() {
                use std::io::Read;
                let mut 缓冲 = Vec::new();
                let _ = e.read_to_end(&mut 缓冲);
                错误输出 = String::from_utf8_lossy(&缓冲).to_string();
            }
            panic!("node 服务进程已退出（退出码 {:?}），地址 {} 未就绪。stderr：\n{}", 码, 地址, 错误输出);
        }
        thread::sleep(Duration::from_millis(200));
    }
    panic!("服务未在超时内就绪: {}", 地址);
}

/// 定位 node 可执行文件
fn node路径() -> String {
    let 候选 = [
        "D:\\program\\nodejs\\node.exe",
        "C:\\program\\nodejs\\node.exe",
        "C:\\Program Files\\nodejs\\node.exe",
        "D:\\Program Files\\nodejs\\node.exe",
    ];
    for c in 候选 {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    // 退而求其次：依赖 PATH（平台化可执行名，POSIX=node / Windows=node.exe）
    base::平台::可执行文件名("node")
}

/// 启动 node 服务（动态空闲端口，彻底避开沙箱内孤儿进程占用的固定端口）
fn 启动服务(脚本: &str, mut envs: Vec<(&str, String)>) -> (服务守卫, u16) {
    // 让 OS 分配一个当前空闲端口，避免与残留 node 进程冲突
    let 监听 = std::net::TcpListener::bind("127.0.0.1:0").expect("无法分配空闲端口");
    let 端口 = 监听.local_addr().unwrap().port();
    drop(监听);
    envs.retain(|(k, _)| *k != "端口");
    envs.push(("端口", 端口.to_string()));
    let 子进程 = Command::new(node路径())
        .arg(脚本)
        .envs(envs.iter().map(|(k, v)| (*k, v.as_str())))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("无法启动 node 服务");
    (服务守卫 { 子进程 }, 端口)
}

#[test]
fn 集成_开发套件客户端对接服务端() {
    if std::env::var("智语言_集成测试").is_err() {
        eprintln!("（跳过集成测试：未设置 智语言_集成测试）");
        return;
    }

    let 工作 = 临时目录();
    // ⚠ 订正（2026-09-20）：`智语言版本文件管理系统` 位于**仓库上一层** `智语言项目\` 下，
    //   而 CARGO_MANIFEST_DIR = `智语言项目\智语言\源码\版本管理器` ⇒ 需 **3 级** `..`
    //   （`源码` → `智语言` → `智语言项目`）。原来只有 2 级 ⇒ 拼出
    //   `智语言\智语言版本文件管理系统\主程序.js`（不存在）⇒ node 报
    //   「Error: Cannot find module …」⇒ 集成测试 2 项全失败（服务端对接门禁 0/2）。
    let 脚本路径 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("智语言版本文件管理系统")
        .join("源码")            // ⚠ 服务端主程序实际在 `源码/主程序.js`（实测 `…\主程序.js` 不存在）
        .join("主程序.js");
    let (mut 服务, 端口) = 启动服务(
        &脚本路径.to_string_lossy(),
        vec![("存储根路径", 工作.to_str().unwrap().to_string())],
    );
    let 地址 = format!("http://127.0.0.1:{}", 端口);
    std::env::set_var("智语言_开发套件镜像源_URL", &地址);
    等待就绪(&地址, &mut 服务.子进程);

    // 创建开发套件夹具：版本列表 + 安装包 + 校验 + 安装程序
    let 版本 = "1.0.0";
    let 平台 = if cfg!(windows) { "windows-x64" } else { "linux-x64" };
    // 列表故意乱序且含更低版本，验证 latest 是按数值比较取最高而非取末位
    fs::write(
        工作.join("版本列表.json"),
        format!("{{\"稳定版\":[\"{}\",\"0.9.12\"],\"测试版\":[]}}", 版本),
    )
    .unwrap();
    let 版本目录 = 工作.join(版本);
    fs::create_dir_all(&版本目录).unwrap();
    let 假包 = format!("智语言开发套件测试包-{}", 版本);
    let 假包字节 = 假包.as_bytes().to_vec();
    let 包名 = format!("智语言开发套件{}-{}.zip", 版本, 平台);
    fs::write(版本目录.join(&包名), &假包字节).unwrap();
    // 校验文件内容（客户端仅原样返回服务端 .sha256 文本，不做本地校验）
    // 注意：服务端工具函数 获取校验文件路径 期望 <包名>.sha256 命名（不含 .zip）
    let 哈希十六进制 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefab";
    fs::write(版本目录.join(format!("{}.sha256", 包名)), format!("{}\n", 哈希十六进制)).unwrap();
    // 安装程序：Windows 为 .exe，其余平台为 .sh（见 工具/文件路径.js）
    let 安装程序名 = if cfg!(windows) {
        format!("智语言开发套件{}.exe", 版本)
    } else {
        format!("智语言开发套件{}.sh", 版本)
    };
    fs::write(版本目录.join(&安装程序名), b"installer").unwrap();
    // 版本详情夹具
    fs::write(
        版本目录.join("版本信息.json"),
        format!(
            "{{\"版本号\":\"{}\",\"发布时间\":\"2026-08-04\",\"说明\":\"集成测试版本\",\"平台列表\":[\"{}\"]}}",
            版本, 平台
        ),
    )
    .unwrap();

    // 查询可安装版本（稳定版）
    let 版本列表 = 开发套件镜像源::查询可安装版本(false).expect("查询可安装版本失败");
    assert!(版本列表.contains(&版本.to_string()), "稳定版应含 {}", 版本);

    // 查询可安装版本（测试版）：夹具中测试版为空，应返回空列表而非报错
    let 测试版列表 = 开发套件镜像源::查询可安装版本(true).expect("查询测试版失败");
    assert!(测试版列表.is_empty(), "测试版应为空：{:?}", 测试版列表);

    // 查询版本详情
    let 详情 = 开发套件镜像源::查询版本详情(版本).expect("查询版本详情失败");
    assert_eq!(详情.版本号, 版本);
    assert_eq!(详情.说明, "集成测试版本");
    assert!(详情.平台列表.contains(&平台.to_string()), "平台列表应含 {}", 平台);
    // 不存在的版本应报错
    assert!(
        开发套件镜像源::查询版本详情("99.99.99").is_err(),
        "不存在的版本应返回错误"
    );

    // 下载工具包（同时校验 sha256）
    let 目标 = 工作.join("下载");
    let (包路径, sha文本) = 开发套件镜像源::下载工具包(版本, &目标).expect("下载工具包失败");
    assert!(包路径.exists(), "安装包文件应存在");
    assert_eq!(sha文本, 哈希十六进制, "校验 sha256 应一致");
    assert_eq!(fs::read(&包路径).unwrap(), 假包字节, "下载内容应与源文件一致");

    // 下载安装程序
    let 安装程序路径 = 开发套件镜像源::下载安装程序(版本, &目标).expect("下载安装程序失败");
    assert!(安装程序路径.exists(), "安装程序文件应存在");
    assert_eq!(fs::read(&安装程序路径).unwrap(), b"installer", "安装程序内容应一致");

    // latest 别名解析：服务端没有名为 latest 的版本，必须解析成真实最高稳定版
    let 解析结果 = 开发套件镜像源::解析版本别名("latest").expect("解析 latest 失败");
    assert_eq!(解析结果, 版本, "latest 应解析为最高稳定版");
    assert_eq!(
        开发套件镜像源::解析版本别名("最新").expect("解析 最新 失败"),
        版本,
        "最新 应与 latest 等价"
    );
    // 非别名原样返回
    assert_eq!(
        开发套件镜像源::解析版本别名("1.2.3").unwrap(),
        "1.2.3",
        "普通版本号应原样返回"
    );
    // 用 latest 直接下载也应成功（内部完成解析）
    let (latest包, _) = 开发套件镜像源::下载工具包("latest", &工作.join("下载latest"))
        .expect("以 latest 下载失败");
    assert!(latest包.exists(), "latest 下载的安装包应存在");

    eprintln!("✅ 开发套件客户端 ↔ 服务端 集成测试通过（5 接口全覆盖）");
}

/// 运行本仓库的 Rust 二进制（智语言 / 智语言开发套件）并合并 stdout+stderr 文本
fn 运行二进制(二进制名: &str, 参数: &[&str], envs: &[(String, String)]) -> String {
    let 路径 = std::env::var(format!("CARGO_BIN_EXE_{}", 二进制名))
        .unwrap_or_else(|_| panic!("未找到二进制 {}", 二进制名));
    let 子进程 = Command::new(路径)
        .args(参数)
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .output()
        .unwrap_or_else(|e| panic!("运行 {} 失败：{}", 二进制名, e));
    let mut 文本 = String::from_utf8_lossy(&子进程.stdout).to_string();
    文本.push_str(&String::from_utf8_lossy(&子进程.stderr).to_string());
    文本
}

/// 命令层对接服务端：验证「智语言」(开发套件系统) 与「智语言开发套件 库」(库管理系统)
/// 的实际 CLI 命令能正确调用两个 Node 服务端（可安装 / 安装 --仅下载 / 库 搜索 / 库 信息）。
/// 动态空闲端口 + TCP 就绪探测，env-gated（智语言_集成测试=1）。
#[test]
fn 集成_命令对接服务端() {
    if std::env::var("智语言_集成测试").is_err() {
        eprintln!("（跳过集成测试：未设置 智语言_集成测试）");
        return;
    }

    // ===== 1. 开发套件系统（智语言 可安装 / 安装 --仅下载）=====
    let 工作 = 临时目录();
    // ⚠ 同订正（2026-09-20）：`智语言版本文件管理系统` 在 `智语言项目\` 下 ⇒ 需 3 级 `..`
    let 脚本路径 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("智语言版本文件管理系统")
        .join("源码")            // ⚠ 同上：主程序在 `源码/主程序.js`
        .join("主程序.js");
    let (mut 开发套件服务, 开发端口) = 启动服务(
        &脚本路径.to_string_lossy(),
        vec![("存储根路径", 工作.to_str().unwrap().to_string())],
    );
    let 开发地址 = format!("http://127.0.0.1:{}", 开发端口);
    等待就绪(&开发地址, &mut 开发套件服务.子进程);

    // 开发套件夹具：版本列表 + 安装包 + 校验 + 安装程序
    let 版本 = "1.0.0";
    let 平台 = if cfg!(windows) { "windows-x64" } else { "linux-x64" };
    fs::write(
        工作.join("版本列表.json"),
        format!("{{\"稳定版\":[\"{}\",\"0.9.12\"],\"测试版\":[]}}", 版本),
    )
    .unwrap();
    let 版本目录 = 工作.join(版本);
    fs::create_dir_all(&版本目录).unwrap();
    let 假包字节 = b"zhilang toolkit package".to_vec();
    let 包名 = format!("智语言开发套件{}-{}.zip", 版本, 平台);
    fs::write(版本目录.join(&包名), &假包字节).unwrap();
    let 哈希十六进制 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefab";
    fs::write(版本目录.join(format!("{}.sha256", 包名)), format!("{}\n", 哈希十六进制)).unwrap();
    let 安装程序名 = if cfg!(windows) {
        format!("智语言开发套件{}.exe", 版本)
    } else {
        format!("智语言开发套件{}.sh", 版本)
    };
    fs::write(版本目录.join(&安装程序名), b"installer").unwrap();
    fs::write(
        版本目录.join("版本信息.json"),
        format!(
            "{{\"版本号\":\"{}\",\"发布时间\":\"2026-08-04\",\"说明\":\"集成测试版本\",\"平台列表\":[\"{}\"]}}",
            版本, 平台
        ),
    )
    .unwrap();

    // 命令：智语言 可安装（命中服务端 版本列表）
    let 可安装文本 = 运行二进制(
        "智语言",
        &["可安装"],
        &[("智语言_开发套件镜像源_URL".to_string(), 开发地址.clone())],
    );
    assert!(
        可安装文本.contains(版本),
        "命令 `可安装` 应列出版本 {}：{}",
        版本,
        可安装文本
    );

    // 命令：智语言 安装 <版本> --仅下载 <临时目录>（命中服务端下载，不改动全局安装）
    let 仅下载目录 = 工作.join("仅下载");
    fs::create_dir_all(&仅下载目录).unwrap();
    let 仅下载文本 = 运行二进制(
        "智语言",
        &["安装", 版本, "--仅下载", &仅下载目录.to_string_lossy()],
        &[("智语言_开发套件镜像源_URL".to_string(), 开发地址.clone())],
    );
    let 仅下载包 = 仅下载目录.join(&包名);
    assert!(
        仅下载包.exists(),
        "命令 `安装 --仅下载` 应把安装包下载到临时目录：{}",
        仅下载文本
    );

    // ===== 2. 库管理系统（智语言开发套件 库 搜索 / 库 信息）=====
    let 库工作 = 临时目录();
    let 库数据库 = 库工作.join("lib.db");
    let 库存储 = 库工作.join("store");
    let 用户名 = format!("rust_cli_{}", std::process::id());
    std::env::set_var("智语言_库用户名", &用户名);
    std::env::set_var("智语言_库密码", "RustTest123");
    std::env::set_var("智语言_库邮箱", format!("{}@example.com", 用户名));

    // ⚠ 同订正（2026-09-20）：`智语言库文件管理系统` 同样在 `智语言项目\` 下 ⇒ 需 3 级 `..`
    let 库脚本路径 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("智语言库文件管理系统")
        .join("源码")            // ⚠ 同上：主程序在 `源码/主程序.js`
        .join("主程序.js");
    let (mut 库服务, 库端口) = 启动服务(
        &库脚本路径.to_string_lossy(),
        vec![
            ("数据库路径", 库数据库.to_str().unwrap().to_string()),
            ("存储根路径", 库存储.to_str().unwrap().to_string()),
        ],
    );
    let 库地址 = format!("http://127.0.0.1:{}", 库端口);
    std::env::set_var("智语言_库镜像源_URL", &库地址);
    等待就绪(&库地址, &mut 库服务.子进程);

    // 封装 + 发布一个库（供 CLI 查询）
    let 库名 = format!("测试库{}", std::process::id());
    let 库版本 = "1.0.0";
    let 项目 = 库工作.join(&库名);
    fs::create_dir_all(项目.join("源码")).unwrap();
    fs::create_dir_all(项目.join("配置")).unwrap();
    fs::write(项目.join("源码").join("主程序.智"), "输出(\"你好，智语言\")\n").unwrap();
    fs::write(
        项目.join("配置").join("项目.配置"),
        format!(
            "[项目]\n名称 = {}\n版本 = {}\n描述 = 集成测试库\n许可证 = MIT\n作者 = {}\n分类 = 工具\n关键词 = 测试\n\n[运行依赖]\n",
            库名, 库版本, 用户名
        ),
    )
    .unwrap();
    let 安装器 = 库安装器类型::新建(项目.clone());
    let 库路径 = 安装器.封装(&项目, false, false).expect("封装失败");
    let 库数据 = fs::read(&库路径).expect("读取 .库 失败");
    镜像源客户端::发布(&库名, 库版本, &库数据).expect("发布失败");

    // 命令：智语言开发套件 库 搜索 <库名>（命中服务端 搜索）
    let 搜索文本 = 运行二进制(
        "智语言开发套件",
        &["库", "搜索", &库名],
        &[("智语言_库镜像源_URL".to_string(), 库地址.clone())],
    );
    assert!(
        搜索文本.contains(&库名),
        "命令 `库 搜索` 应列出本库：{}",
        搜索文本
    );

    // 命令：智语言开发套件 库 信息 <库名>（命中服务端 库信息）
    let 信息文本 = 运行二进制(
        "智语言开发套件",
        &["库", "信息", &库名],
        &[("智语言_库镜像源_URL".to_string(), 库地址.clone())],
    );
    assert!(
        信息文本.contains(&库名),
        "命令 `库 信息` 应含库名：{}",
        信息文本
    );

    eprintln!("✅ 命令层对接服务端 集成测试通过（开发套件 可安装/安装 --仅下载；库 搜索/信息）");
}
