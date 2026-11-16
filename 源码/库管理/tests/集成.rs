// 库管理系统 客户端 ↔ 服务端 集成测试（env-gated：需设置 智语言_集成测试=1 且本机有 node）
// 运行：智语言_集成测试=1 cargo test -p libmgr --test 集成 -- --nocapture
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

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
    let 名 = format!("zhilang_lib_test_{}_{}", std::process::id(), 纳秒);
    let p = 基.join(名);
    fs::create_dir_all(&p).unwrap();
    p
}

/// 就绪探测用 TCP 连接而非 HTTP 请求：沙箱内 ureq 走代理探测会挂起，
/// 直接 TcpStream::connect 到监听端口是最可靠的"服务已起来"信号。
fn 等待就绪(端口: u16, 子进程: &mut Child) {
    let 目标 = format!("127.0.0.1:{}", 端口);
    for _ in 0..100 {
        if std::net::TcpStream::connect_timeout(
            &目标.parse().unwrap(),
            Duration::from_millis(500),
        )
        .is_ok()
        {
            return;
        }
        // 若 node 进程已退出，立即报错并暴露 stderr
        if let Ok(Some(码)) = 子进程.try_wait() {
            let mut 错误输出 = String::new();
            if let Some(mut e) = 子进程.stderr.take() {
                use std::io::Read;
                let mut 缓冲 = Vec::new();
                let _ = e.read_to_end(&mut 缓冲);
                错误输出 = String::from_utf8_lossy(&缓冲).to_string();
            }
            panic!("node 服务进程已退出（退出码 {:?}），端口 {} 未就绪。stderr：\n{}", 码, 端口, 错误输出);
        }
        thread::sleep(Duration::from_millis(200));
    }
    panic!("服务未在超时内就绪，端口: {}", 端口);
}

/// 定位 node 可执行文件（cargo 子进程可能未继承交互式 PATH，故显式探测）
fn node路径() -> String {
    // 常见安装位置
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
    let mut 子进程 = Command::new(node路径())
        .arg(脚本)
        .envs(envs.iter().map(|(k, v)| (*k, v.as_str())))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("无法启动 node 服务（请确认本机已安装 node 且在 PATH 中）");
    thread::sleep(Duration::from_millis(1000));
    if let Ok(Some(_)) = 子进程.try_wait() {
        let mut 错误输出 = String::new();
        if let Some(mut e) = 子进程.stderr.take() {
            use std::io::Read;
            let mut 缓冲 = Vec::new();
            let _ = e.read_to_end(&mut 缓冲);
            错误输出 = String::from_utf8_lossy(&缓冲).to_string();
        }
        panic!("node 服务启动失败：{}", 错误输出);
    }
    (服务守卫 { 子进程 }, 端口)
}

#[test]
fn 集成_库管理客户端对接服务端() {
    if std::env::var("智语言_集成测试").is_err() {
        eprintln!("（跳过集成测试：未设置 智语言_集成测试）");
        return;
    }

    let 工作 = 临时目录();
    let 数据库 = 工作.join("lib.db");
    let 存储 = 工作.join("store");

    let 用户名 = format!("rust_test_{}", std::process::id());
    std::env::set_var("智语言_库用户名", &用户名);
    std::env::set_var("智语言_库密码", "RustTest123");
    std::env::set_var("智语言_库邮箱", format!("{}@example.com", 用户名));

    // ⚠ 订正（2026-09-20）—— **两处**：
    //   ① 层数：`智语言库文件管理系统` 位于**仓库上一层** `智语言项目\` 下，
    //      而 CARGO_MANIFEST_DIR = `智语言项目\智语言\源码\库管理` ⇒ 需 **3 级** `..`
    //      （`源码` → `智语言` → `智语言项目`）；原来只有 2 级 ⇒ 拼出
    //      `智语言\智语言库文件管理系统\主程序.js`（不存在）。
    //   ② 子目录：服务端主程序实际在 **`源码/主程序.js`**（实测 `…\主程序.js` 不存在）。
    //   两处叠加 ⇒ node 报「Error: Cannot find module …」⇒ 集成测试 11 接口一项整体失败。
    let 脚本路径 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("智语言库文件管理系统")
        .join("源码")
        .join("主程序.js");
    let (mut _服务, 端口) = 启动服务(
        &脚本路径.to_string_lossy(),
        vec![
            ("数据库路径", 数据库.to_str().unwrap().to_string()),
            ("存储根路径", 存储.to_str().unwrap().to_string()),
        ],
    );
    let 地址 = format!("http://127.0.0.1:{}", 端口);
    // 客户端在每次调用时读取此环境变量，启动后再设置即可
    std::env::set_var("智语言_库镜像源_URL", &地址);
    等待就绪(端口, &mut _服务.子进程);

    // 构造一个合法库项目
    let 库名 = format!("测试库{}", std::process::id());
    let 版本 = "1.0.0";
    let 项目 = 工作.join(&库名);
    fs::create_dir_all(项目.join("源码")).unwrap();
    fs::create_dir_all(项目.join("配置")).unwrap();
    fs::write(项目.join("源码").join("主程序.智"), "输出(\"你好，智语言\")\n").unwrap();
    fs::write(
        项目.join("配置").join("项目.配置"),
        format!(
            "[项目]\n名称 = {}\n版本 = {}\n描述 = 集成测试库\n许可证 = MIT\n作者 = {}\n分类 = 工具\n关键词 = 测试\n\n[运行依赖]\n",
            库名, 版本, 用户名
        ),
    )
    .unwrap();

    // 封装 + 发布（发布会自举：登录 → 失败则注册+激活+登录）
    let 安装器 = 库安装器类型::新建(项目.clone());
    let 库路径 = 安装器.封装(&项目, false, false).expect("封装失败");
    let 库数据 = fs::read(&库路径).expect("读取 .库 失败");
    镜像源客户端::发布(&库名, 版本, &库数据).expect("发布失败");

    // 搜索
    let 搜索结果 = 镜像源客户端::搜索(&库名).expect("搜索失败");
    assert!(搜索结果.iter().any(|x| x.名称 == 库名), "搜索应返回本库: {:?}", 搜索结果);

    // 查询版本
    let 版本列表 = 镜像源客户端::查询版本(&库名).expect("查询版本失败");
    assert!(版本列表.contains(&版本.to_string()), "版本列表应含 {}", 版本);

    // 查询库信息
    let 信息 = 镜像源客户端::查询库信息(&库名).expect("查询库信息失败");
    assert_eq!(信息.版本, vec![版本.to_string()]);
    assert_eq!(信息.许可证, "MIT");

    // 下载库
    let 下载 = 镜像源客户端::下载库(&库名, 版本).expect("下载库失败");
    assert!(下载.contains_key(&库名), "下载结果应含本库");
    assert!(下载.get(&库名).unwrap().len() > 0);

    // 查询依赖声明
    let 依赖列表 = 镜像源客户端::查询依赖(&库名).expect("查询依赖失败");
    assert!(
        依赖列表.iter().any(|d| d.版本号 == 版本),
        "依赖列表应含版本 {}：{:?}",
        版本,
        依赖列表
    );

    // 验证令牌：有效令牌 → 有效且用户名匹配；伪造令牌 → 无效
    let 令牌 = 镜像源客户端::获取会话令牌().expect("获取会话令牌失败");
    let (有效, 令牌用户名) = 镜像源客户端::验证令牌(&令牌).expect("验证令牌失败");
    assert!(有效, "刚签发的令牌应有效");
    assert_eq!(令牌用户名, 用户名, "令牌应归属当前用户");
    let (伪造有效, _) = 镜像源客户端::验证令牌("这是一个不存在的令牌").expect("验证伪造令牌请求失败");
    assert!(!伪造有效, "伪造令牌应判定无效");

    // —— 端到端：安装器从镜像源装库到全新项目，并写出锁文件 ——
    let 消费项目 = 工作.join("消费方");
    fs::create_dir_all(消费项目.join("配置")).unwrap();
    fs::write(
        消费项目.join("配置").join("项目.配置"),
        "[项目]\n名称 = 消费方\n版本 = 0.1.0\n",
    )
    .unwrap();
    let 消费安装器 = 库安装器类型::新建(消费项目.clone());
    消费安装器.安装(&库名, None).expect("从镜像源安装库失败");

    // 源码应落到 库/<库名><版本>/源码/
    let 装入源码 = 消费项目
        .join("库")
        .join(format!("{}{}", 库名, 版本))
        .join("源码");
    assert!(装入源码.exists(), "安装后源码目录应存在：{:?}", 装入源码);
    let 装入文件 = fs::read_dir(&装入源码)
        .unwrap()
        .filter_map(|e| e.ok())
        .count();
    assert!(装入文件 > 0, "安装后源码目录不应为空");

    // 锁文件应记录该库与版本
    let 锁路径 = 消费项目.join("配置").join("项目依赖.配置");
    assert!(锁路径.exists(), "安装后应生成锁文件 项目依赖.配置");
    let 锁内容 = fs::read_to_string(&锁路径).unwrap();
    assert!(锁内容.contains(&库名), "锁文件应含库名：{}", 锁内容);
    assert!(锁内容.contains(版本), "锁文件应含版本号：{}", 锁内容);

    // 弃用版本（作者本人操作应成功）——放在安装之后，避免装到已弃用版本
    镜像源客户端::弃用版本(&库名, 版本, "", "集成测试弃用").expect("弃用版本失败");
    // 重复弃用应被服务端拒绝
    assert!(
        镜像源客户端::弃用版本(&库名, 版本, "", "重复弃用").is_err(),
        "重复弃用应返回错误"
    );

    // 登出：注销后同一令牌应失效
    镜像源客户端::登出(&令牌).expect("登出失败");
    let (登出后有效, _) = 镜像源客户端::验证令牌(&令牌).expect("登出后验证令牌请求失败");
    assert!(!登出后有效, "登出后令牌应失效");

    eprintln!("✅ 库管理客户端 ↔ 服务端 集成测试通过（11 接口全覆盖）");
}
