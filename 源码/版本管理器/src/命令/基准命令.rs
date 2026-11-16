// 文件: 版本管理器/src/命令/基准命令.rs
// 职责: 性能基准测试运行器，运行 基准* 函数并统计耗时
//
// 双模约定（与 测试 命令一致）：
//   - `智语言 基准 <file>`            ：解释模式（默认，不加参数），VM 逐函数计时
//   - `智语言 基准 <file> --编译模式` ：编译模式，合成计时 启动程序 → 编 exe → 解析耗时

use crate::参数解析;
use crate::测试辅助;
use pm::项目检测;
use pm::项目检测::项目归属;
use frontend::{
    诊断收集器::诊断收集器,
    流水线::输出诊断,
    词法分析::词法分析器,
    语法分析::{语法分析器, 顶层定义},
    语义::语义分析器,
    降级::公共降级,
};
use interpreter::{字节码生成::字节码生成器, 虚拟机::虚拟机};
use std::fs;
use std::time::Instant;

pub fn 执行(参数: &[String]) {
    let 文件路径 = 参数解析::提取文件路径(参数);
    let 过滤关键词 = 参数解析::获取标志值(参数, "--过滤");
    let 重复次数: u32 = 参数解析::获取标志值(参数, "--重复")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let 是否编译 = 参数解析::有标志(参数, "--编译模式");
    let 是否双模式 = 参数解析::有标志(参数, "--双模式");

    match 文件路径 {
        Some(路径) => {
            // 有文件参数：单文件基准
            执行单文件基准(&路径, 过滤关键词, 重复次数, 是否编译, 是否双模式);
        }
        None => {
            // 无文件参数：必须在项目内
            match 项目检测::检测当前目录项目归属() {
                项目归属::项目内(_根目录) => {
                    let 文件列表 = 扫描基准文件();
                    if 文件列表.is_empty() {
                        eprintln!("[智语言] 错误：未找到包含基准测试的 .智 文件");
                        return;
                    }
                    for 文件 in &文件列表 {
                        执行单文件基准(文件, 过滤关键词.clone(), 重复次数, 是否编译, 是否双模式);
                    }
                }
                项目归属::项目外 => {
                    eprintln!("[智语言] 错误：未找到 配置/项目.配置，基准命令需要在项目目录下执行");
                }
            }
        }
    }
}

/// 执行单个文件的基准测试（解释/编译/双模式）
fn 执行单文件基准(文件路径: &str, 过滤: Option<String>, 重复次数: u32, 是否编译: bool, 是否双模式: bool) {
    let 基准列表 = 提取基准函数(文件路径, 过滤);
    if 基准列表.is_empty() { return; }

    if 是否双模式 {
        // —— 双模式：同时跑解释+编译，显示加速比 ——
        let 解释结果 = 运行基准文件_解释(文件路径, &基准列表, 重复次数);
        let 编译结果 = 运行基准文件_编译(文件路径, &基准列表, 重复次数);
        eprintln!("[智语言] 基准测试（双模式）");
        let 最大 = 解释结果.len().max(编译结果.len());
        for i in 0..最大 {
            let (名称, 解释耗时) = 解释结果.get(i)
                .map(|(n, a, _)| (n.as_str(), a.as_nanos()))
                .unwrap_or(("", 0));
            let 编译耗时 = 编译结果.get(i)
                .map(|(_, a, _)| a.as_nanos())
                .unwrap_or(0);
            let 加速比文本 = if 编译耗时 > 0 && 解释耗时 > 0 {
                format!("{:>5.1}x", 解释耗时 as f64 / 编译耗时 as f64)
            } else { "  N/A".to_string() };
            eprintln!("  {}  解释 {:>8.2?}  编译 {:>8.2?}  加速比 {}",
                名称,
                解释结果.get(i).map(|(_, a, _)| *a).unwrap_or_default(),
                编译结果.get(i).map(|(_, a, _)| *a).unwrap_or_default(),
                加速比文本);
        }
    } else if 是否编译 {
        // —— 编译模式 ——
        let 结果 = 运行基准文件_编译(文件路径, &基准列表, 重复次数);
        eprintln!("[智语言] 基准测试完成（编译模式）");
        for (名称, 平均, _) in &结果 {
            eprintln!("  {}  {:>10.2?}", 名称, 平均);
        }
    } else {
        // —— 解释模式（默认）——
        let 结果 = 运行基准文件_解释(文件路径, &基准列表, 重复次数);
        eprintln!("[智语言] 基准测试完成");
        for (名称, 平均, _) in &结果 {
            eprintln!("  {}  {:>10.2?}", 名称, 平均);
        }
    }
}

/// 从源码提取以"基准"开头的函数名列表（供解释/编译两路径共用）
fn 提取基准函数(文件路径: &str, 过滤: Option<String>) -> Vec<String> {
    let 源码 = match fs::read_to_string(文件路径) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[智语言] 错误：无法读取文件 '{}': {}", 文件路径, e);
            return Vec::new();
        }
    };
    let mut 诊断 = 诊断收集器::新建();
    frontend::诊断全局::设置诊断模式_自CLI(&mut 诊断);
    诊断.注册源文件(文件路径, &源码);
    let 词条列表 = 词法分析器::新建(&源码, 文件路径.to_string(), &mut 诊断).扫描所有词条();
    if 诊断.有错误() { 输出诊断(&诊断); return Vec::new(); }
    let 程序 = 语法分析器::新建(&词条列表, &mut 诊断).解析程序();
    if 诊断.有错误() { 输出诊断(&诊断); return Vec::new(); }

    let mut 基准列表 = Vec::new();
    for 定义 in &程序.顶层定义列表 {
        if let 顶层定义::函数定义(函数) = 定义 {
            if 函数.名称.starts_with("基准") {
                if let Some(ref 过滤词) = 过滤 {
                    if !函数.名称.contains(过滤词.as_str()) { continue; }
                }
                基准列表.push(函数.名称.clone());
            }
        }
    }
    基准列表
}

/// 解释模式：VM 逐函数计时（含预热 3 次）
fn 运行基准文件_解释(文件路径: &str, 基准函数列表: &[String], 重复次数: u32) -> Vec<(String, std::time::Duration, std::time::Duration)> {
    let 源码 = match fs::read_to_string(文件路径) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut 诊断 = 诊断收集器::新建();
    frontend::诊断全局::设置诊断模式_自CLI(&mut 诊断);
    let 词条列表 = 词法分析器::新建(&源码, 文件路径.to_string(), &mut 诊断).扫描所有词条();
    if 诊断.有错误() { return Vec::new(); }
    let mut 程序 = 语法分析器::新建(&词条列表, &mut 诊断).解析程序();
    if 诊断.有错误() { return Vec::new(); }

    let mut 语义器 = 语义分析器::新建(&mut 诊断);
    let 高层表示模块 = match 语义器.分析程序(&mut 程序) {
        Ok(m) => m, Err(_) => { return Vec::new(); }
    };
    let 底层表示模块 = 公共降级::降级转换(&高层表示模块);
    let 字节码 = match 字节码生成器::新建().生成(&底层表示模块) {
        Ok(b) => b,
        Err(e) => { eprintln!("[智语言] 字节码生成失败: {}", e); return Vec::new(); }
    };

    // 🆕 预热 3 次（不计入结果）
    for 函数名 in 基准函数列表 {
        for _ in 0..3 {
            let mut 预热虚拟机 = 虚拟机::新建();
            预热虚拟机.加载字节码(&字节码);
            let _ = 预热虚拟机.调用函数(函数名);
        }
    }

    let mut 结果列表 = Vec::new();
    for 函数名 in 基准函数列表 {
        let mut 总耗时 = std::time::Duration::from_secs(0);
        let mut 成功次数 = 0u32;
        for _ in 0..重复次数 {
            let 开始 = Instant::now();
            let mut 虚拟机实例 = 虚拟机::新建();
            虚拟机实例.加载字节码(&字节码);
            match 虚拟机实例.调用函数(函数名) {
                Ok(_) => {
                    总耗时 += 开始.elapsed();
                    成功次数 += 1;
                }
                Err(_e) => { break; }
            }
        }
        if 成功次数 > 0 {
            let 平均耗时 = 总耗时 / 成功次数;
            结果列表.push((函数名.clone(), 平均耗时, 总耗时));
        }
    }
    结果列表
}

/// 编译模式：合成基准夹具 → 编译 exe → 执行（含预热 1 次编译）
fn 运行基准文件_编译(文件路径: &str, 基准函数列表: &[String], 重复次数: u32) -> Vec<(String, std::time::Duration, std::time::Duration)> {
    if 基准函数列表.is_empty() { return Vec::new(); }

    let 合成源码 = 测试辅助::合成基准启动程序(基准函数列表, 重复次数);
    let 临时路径 = 测试辅助::写出临时夹具(文件路径, &合成源码);

    // 🆕 预热 1 次（使 CPU/OS 页缓存预热）
    let (_, _) = 测试辅助::编译并执行(&临时路径);

    let (文本, 退出码) = 测试辅助::编译并执行(&临时路径);
    测试辅助::清理临时夹具(&临时路径);

    if 退出码 != 0 {
        eprintln!("[智语言] 错误：基准编译/运行失败，退出码={}", 退出码);
        eprintln!("{}", 文本);
        return Vec::new();
    }
    let mut 结果列表 = Vec::new();
    for 名 in 基准函数列表 {
        let 前缀 = format!("{} ", 名);
        for 行 in 文本.lines() {
            if 行.starts_with(&前缀) {
                let 耗时文本 = 行[前缀.len()..].trim().to_string();
                eprintln!("  {}  {}", 名, 耗时文本);
                结果列表.push((名.clone(), std::time::Duration::from_millis(0), std::time::Duration::from_millis(0)));
            }
        }
    }
    结果列表
}

/// 扫描当前目录和测试目录下的 .智 文件
fn 扫描基准文件() -> Vec<String> {
    let mut 文件列表 = Vec::new();
    for 目录 in &[".".to_string(), "测试".to_string()] {
        if let Ok(条目) = std::fs::read_dir(目录) {
            for 条目 in 条目.flatten() {
                let 路径 = 条目.path();
                if 路径.extension().map_or(false, |e| e == "智") {
                    if let Some(名称) = 路径.to_str() {
                        // 简单过滤：快速检查文件名或内容是否包含"基准"
                        if 名称.contains("基准") {
                            文件列表.push(名称.to_string());
                            continue;
                        }
                        // 也检查文件内容
                        if let Ok(内容) = fs::read_to_string(&路径) {
                            if 内容.contains("基准") {
                                文件列表.push(名称.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    文件列表
}
