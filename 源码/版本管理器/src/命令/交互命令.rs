// 文件: 版本管理器/src/命令/交互命令.rs
// 职责: REPL 交互式控制台，逐行输入执行智语言代码
//       退出时自动清理临时缓存

use pm::目录;
use frontend::{
    诊断收集器::诊断收集器,
    流水线::输出诊断,
    词法分析::词法分析器,
    语法分析::语法分析器,
    语义::语义分析器,
    降级::公共降级,
};
use interpreter::{
    字节码生成::字节码生成器,
    虚拟机::虚拟机,
};
use std::io::{self, Write};

pub fn 执行(_参数: &[String]) {
    eprintln!("智语言 交互模式（输入 .退出 结束）");

    let mut 虚拟机 = 虚拟机::新建();
    let mut 行号 = 1u64;
    // 交互模式临时缓存目录（退出时自动清理）
    let 临时缓存 = 目录::全局字节码目录().join("交互");

    loop {
        print!("> ");
        if io::stdout().flush().is_err() {
            break;
        }

        let mut 输入 = String::new();
        match io::stdin().read_line(&mut 输入) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(_) => break,
        }

        let 输入 = 输入.trim().to_string();
        if 输入.is_empty() {
            continue;
        }

        // 处理特殊命令
        if 输入.starts_with('.') {
            match 输入.as_str() {
                ".退出" | ".quit" | ".q" => break,
                ".帮助" | ".help" | ".h" => {
                    eprintln!("特殊命令:");
                    eprintln!("  .退出, .quit, .q    退出交互控制台");
                    eprintln!("  .帮助, .help, .h    显示此帮助");
                    eprintln!("  .清空, .clear      清空虚拟机状态");
                    continue;
                }
                ".清空" | ".clear" => {
                    虚拟机 = 虚拟机::新建();
                    eprintln!("状态已清空");
                    continue;
                }
                _ => {
                    eprintln!("未知命令: {}。输入 '.帮助' 查看可用命令。", 输入);
                    continue;
                }
            }
        }

        // 正常的智语言代码
        let 源码 = 输入.clone();
        let 文件路径 = format!("<交互#{}>", 行号);
        行号 += 1;

        let mut 诊断 = 诊断收集器::新建();
        frontend::诊断全局::设置诊断模式_自CLI(&mut 诊断);

        // 词法分析
        let 词条列表 = 词法分析器::新建(&源码, 文件路径.clone(), &mut 诊断).扫描所有词条();
        if 诊断.有错误() {
            输出诊断(&诊断);
            continue;
        }

        // 语法分析
        let mut 程序 = 语法分析器::新建(&词条列表, &mut 诊断).解析程序();
        if 诊断.有错误() {
            输出诊断(&诊断);
            continue;
        }

        // 语义分析
        let 高层表示模块 = match 语义分析器::新建(&mut 诊断).分析程序(&mut 程序) {
            Ok(m) => m,
            Err(_) => {
                输出诊断(&诊断);
                continue;
            }
        };

        // 降级为LIR
        let 底层表示模块 = 公共降级::降级转换(&高层表示模块);

        // 生成字节码并执行
        let 字节码 = match 字节码生成器::新建().生成(&底层表示模块) {
            Ok(b) => b,
            Err(e) => { eprintln!("[智语言] 字节码生成失败: {}", e); continue; }
        };
        虚拟机.加载字节码(&字节码);
        虚拟机.执行();
    }

    // 退出前自动清理临时缓存
    if 临时缓存.exists() {
        let _ = std::fs::remove_dir_all(&临时缓存);
    }
    eprintln!("[智语言] 已退出");
}

