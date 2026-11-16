// 文件: 版本管理器/src/命令/测试辅助.rs
// 职责: 测试 / 基准 命令共用的「编译模式」底层能力：
//   1. 为「仅含 测试X/基准X 函数、无 启动程序」的文件合成 启动程序 夹具
//   2. 复用 构建运行 路径把夹具/原文件编译为原生 exe 并 exec 一次，捕获 stdout+stderr
// 双模统一点：解释模式各自用 VM 逐函数；编译模式都收敛到
//   「注入/复用 启动程序 → 编译成 exe → exec 一次 → 解析 stdout」三段式。

use std::fs;
use std::path::Path;
use std::process::Command;

/// 合成测试夹具的 启动程序。
/// - 若原文件已含 启动程序，直接返回原源码（复用用户编排）；
/// - 否则在末尾注入一个伪 启动程序：逐个 捕获: 测试X() 异常: 输出("失败 测试X - ...") 结束，
///   隔离每个测试的异常，使编译模式产出与解释模式逐函数判定口径一致。
pub fn 合成测试启动程序(测试函数列表: &[String], 含启动程序: bool, 原源码: &str) -> String {
    if 含启动程序 {
        return 原源码.to_string();
    }
    let mut s = 原源码.to_string();
    s.push_str("\n启动程序():\n");
    for 名 in 测试函数列表 {
        s.push_str(&format!(
            "    捕获:\n        {}()\n    异常:\n        输出(\"失败 {} - 该测试抛出异常\")\n    结束\n",
            名, 名
        ));
    }
    s.push_str("结束\n");
    s
}

/// 合成基准夹具的 启动程序（总是合成，忽略原文件是否已含 启动程序）：
/// 对每个 基准X 函数做 重复次数 次计时循环，输出 "基准X  <平均耗时(ms)>"。
pub fn 合成基准启动程序(基准函数列表: &[String], 重复次数: u32) -> String {
    let mut s = String::new();
    s.push_str("启动程序():\n");
    for 名 in 基准函数列表 {
        let 计时 = format!("计时{}", 名);
        s.push_str(&format!(
            "    {} = 0\n    循环 计数 = 1, 计数 <= {}, 计数 += 1:\n        开始 = 时间.当前时间()\n        {}()\n        {} += 时间.当前时间() - 开始\n    结束\n    输出(\"{}  \" + 文本({} / {}))\n",
            计时, 重复次数, 名, 计时, 名, 计时, 重复次数
        ));
    }
    s.push_str("结束\n");
    s
}

/// 把合成源码写到「与原文件同目录」的临时 .智（保留相对导入解析），返回临时路径。
/// 调用方应在使用完毕后删除该文件（见 清理临时夹具）。
pub fn 写出临时夹具(原路径: &str, 合成源码: &str) -> String {
    let 原 = Path::new(原路径);
    let 目录 = 原.parent().unwrap_or_else(|| Path::new("."));
    let 词干 = 原.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "夹具".to_string());
    let 临时名 = format!("{}_测试夹具.智", 词干);
    let 临时路径 = 目录.join(&临时名);
    let _ = fs::write(&临时路径, 合成源码);
    临时路径.to_string_lossy().to_string()
}

/// 删除临时夹具文件（忽略错误）。
pub fn 清理临时夹具(临时路径: &str) {
    let _ = fs::remove_file(临时路径);
}

/// 编译并执行：复用 构建运行 路径把指定 .智 编译为原生 exe 并运行一次，
/// 返回 (stdout+stderr 文本, 退出码)。
/// 执行后 touch 静态库（mtime 晚于 exe），避免后续 构建运行 触发 >120s 全量重建。
/// 🆕 方案B：子进程带超时（默认 120s）。超时则强制终止，避免环境抖动/离线导致
/// 进程挂起遗留僵尸锁（见记忆「PID9036 挂起进程锁文件」）。
pub fn 编译并执行(路径: &str) -> (String, i32) {
    let 自身exe = std::env::current_exe().unwrap_or_default();
    let mut 命令 = Command::new(&自身exe);
    命令.arg("构建运行").arg(路径).arg("--重新编译");
    // 已是工具包本体，置位防递归（即便被再次转发也无副作用）
    命令.env("智语言_已引导", "1");
    // 允许子进程继承 stdin 之外不阻塞；输出整体捕获
    命令.stdout(std::process::Stdio::piped());
    命令.stderr(std::process::Stdio::piped());

    let 子进程 = match 命令.spawn() {
        Ok(p) => p,
        Err(e) => return (format!("编译执行失败: {}", e), -1),
    };

    // 在独立线程等待子进程退出并捕获 stdout/stderr，主线程带超时 recv_timeout。
    // 子进程句柄放 Arc<Mutex<Option<Child>>>，超时分支可 take 出并强制终止。
    let 子进程共享 = std::sync::Arc::new(std::sync::Mutex::new(Some(子进程)));
    let 子进程克隆 = 子进程共享.clone();
    let (发送端, 接收端) = std::sync::mpsc::channel();
    let _等待线程 = std::thread::spawn(move || {
        let mut 守卫 = 子进程克隆.lock().unwrap();
        if let Some(子) = 守卫.take() {
            drop(守卫);
            let 结果 = 子.wait_with_output();
            let _ = 发送端.send(结果);
        }
    });
    const 超时毫秒: u64 = 120_000; // 与门禁每例 2 分钟一致
    let 输出 = match 接收端.recv_timeout(std::time::Duration::from_millis(超时毫秒)) {
        Ok(结果) => 结果,
        Err(_) => {
            // 超时：强制终止挂起子进程并清理，返回明确失败（不 todo!()，离线明确报错）
            let mut 守卫 = 子进程共享.lock().unwrap();
            if let Some(mut 残留) = 守卫.take() {
                let _ = 残留.kill();
                let _ = 残留.wait();
            }
            return (format!("编译执行超时（>{} 毫秒）：{}", 超时毫秒, 路径), -1);
        }
    };

    // 编译后把静态库 mtime 设为最新，保证后续 构建运行 走增量而非全量重建
    if let Some(父) = 自身exe.parent() {
        let 库 = 父.join("base.lib");
        if 库.exists() {
            if let Ok(文件) = std::fs::OpenOptions::new().write(true).open(&库) {
                let _ = 文件.set_modified(std::time::SystemTime::now());
            }
        }
    }

    match 输出 {
        Ok(o) => {
            let mut 文本 = String::new();
            文本.push_str(&String::from_utf8_lossy(&o.stdout));
            文本.push_str(&String::from_utf8_lossy(&o.stderr));
            let 码 = o.status.code().unwrap_or(-1);
            (文本, 码)
        }
        Err(e) => (format!("编译执行失败: {}", e), -1),
    }
}
