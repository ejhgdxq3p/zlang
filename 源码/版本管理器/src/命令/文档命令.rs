// 文件: 版本管理器/src/命令/文档命令.rs
// 职责: 从源代码提取文档注释生成 API 文档

use crate::参数解析;
use frontend::{
    诊断收集器::诊断收集器,
    文档生成器::{文档生成器, 文档节点},
};
use std::fs;
use std::path::PathBuf;
use pm::{项目检测, 项目检测::项目归属};

pub fn 执行(参数: &[String]) {
    // 无文件参数 → 生成整个项目的 API 文档
    if 参数解析::提取文件路径(参数).is_none() {
        执行项目文档();
        return;
    }
    let 文件路径 = 参数解析::提取文件路径(参数).unwrap();

    let 源码 = match fs::read_to_string(&文件路径) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[智语言] 错误：无法读取文件 '{}': {}", 文件路径, e);
            return;
        }
    };

    let mut 诊断 = 诊断收集器::新建();
    frontend::诊断全局::设置诊断模式_自CLI(&mut 诊断);
    let mut 生成器 = 文档生成器::新建(&mut 诊断);

    let 文档节点列表 = 生成器.提取文档(&源码, &文件路径);

    if 文档节点列表.is_empty() {
        eprintln!("[智语言] 错误：未找到文档注释 (//** ... **/)。");
        return;
    }

    println!("# API 文档: {}", 文件路径);
    println!();

    for 节点 in &文档节点列表 {
        打印文档节点(节点, 0);
    }

    // 也输出 Markdown 格式
    let 模块名 = 文件路径.trim_end_matches(".智");
    let 渲染文本 = 生成器.渲染标记文档(&文档节点列表, 模块名);
    if !渲染文本.is_empty() {
        println!();
        println!("---");
        println!("{}", 渲染文本);
    }

    let 绝对路径 = std::fs::canonicalize(&文件路径).unwrap_or_else(|_| std::path::PathBuf::from(&文件路径));
    eprintln!("[智语言] 文档已生成：{}.md", 绝对路径.display());
}

fn 打印文档节点(节点: &文档节点, 缩进级别: usize) {
    let 缩进 = "  ".repeat(缩进级别);

    match 节点 {
        文档节点::模块 { 名称, 描述 } => {
            println!("{}## 模块: {}", 缩进, 名称);
            if !描述.is_empty() {
                println!("{}{}", 缩进, 描述);
            }
            println!();
        }
        文档节点::函数 { 名称, 描述, 形参文档, 返回描述 } => {
            println!("{}### 函数: `{}`", 缩进, 名称);
            if !描述.is_empty() {
                println!("{}{}", 缩进, 描述);
            }
            if !形参文档.is_empty() {
                println!("{}参数:", 缩进);
                for (参数名, 参数描述) in 形参文档 {
                    println!("{}  - `{}`: {}", 缩进, 参数名, 参数描述);
                }
            }
            if !返回描述.is_empty() {
                println!("{}*返回值*: {}", 缩进, 返回描述);
            }
            println!();
        }
        文档节点::结构体 { 名称, 描述, 字段文档 } => {
            println!("{}### 结构体: `{}`", 缩进, 名称);
            if !描述.is_empty() {
                println!("{}{}", 缩进, 描述);
            }
            if !字段文档.is_empty() {
                println!("{}字段:", 缩进);
                for (字段名, 类型, 字段描述) in 字段文档 {
                    println!("{}  - `{}`: {} — {}", 缩进, 字段名, 类型, 字段描述);
                }
            }
            println!();
        }
        文档节点::枚举 { 名称, 描述, 成员文档 } => {
            println!("{}### 枚举: `{}`", 缩进, 名称);
            if !描述.is_empty() {
                println!("{}{}", 缩进, 描述);
            }
            if !成员文档.is_empty() {
                println!("{}成员:", 缩进);
                for (成员名, 成员描述) in 成员文档 {
                    println!("{}  - `{}`: {}", 缩进, 成员名, 成员描述);
                }
            }
            println!();
        }
        文档节点::变量 { 名称, 描述 } => {
            println!("{}### 变量: `{}`", 缩进, 名称);
            if !描述.is_empty() {
                println!("{}{}", 缩进, 描述);
            }
            println!();
        }
        文档节点::接口 { 名称, 描述 } => {
            println!("{}### 接口: `{}`", 缩进, 名称);
            if !描述.is_empty() {
                println!("{}{}", 缩进, 描述);
            }
            println!();
        }
    }
}

/// 项目级文档：扫描 源码/**/*.智，聚合所有文档注释，输出 文档/<项目名>.md
fn 执行项目文档() {
    let 归属 = 项目检测::检测当前目录项目归属();
    let 根目录 = match 归属 {
        项目归属::项目内(根) => 根,
        项目归属::项目外 => {
            eprintln!("[智语言] 错误：用法: 智语言 文档 <文件.智> 或在项目目录内运行 `智语言 文档`");
            return;
        }
    };

    let 源码目录 = 根目录.join("源码");
    let 文件列表: Vec<PathBuf> = if 源码目录.exists() {
        递归收集智文件(&源码目录)
    } else {
        递归收集智文件(&根目录)
    };
    if 文件列表.is_empty() {
        eprintln!("[智语言] 错误：未找到任何 .智 源文件");
        return;
    }

    let mut 诊断 = 诊断收集器::新建();
    frontend::诊断全局::设置诊断模式_自CLI(&mut 诊断);
    let mut 生成器 = 文档生成器::新建(&mut 诊断);

    let mut 总文档: Vec<文档节点> = Vec::new();
    let mut 标记段落 = String::new();
    for 文件 in &文件列表 {
        let 源码 = match fs::read_to_string(文件) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let 节点 = 生成器.提取文档(&源码, &文件.display().to_string());
        if 节点.is_empty() {
            continue;
        }
        let 模块名 = 文件.display().to_string();
        let 渲染 = 生成器.渲染标记文档(&节点, &模块名);
        标记段落.push_str(&渲染);
        标记段落.push_str("\n\n");
        总文档.extend(节点);
    }

    if 总文档.is_empty() {
        eprintln!("[智语言] 错误：未找到任何文档注释 (//** ... **/)");
        return;
    }

    let 项目名 = 根目录.file_name().and_then(|n| n.to_str()).unwrap_or("项目").to_string();
    let 文档目录 = 根目录.join("文档");
    let _ = fs::create_dir_all(&文档目录);
    let 输出路径 = 文档目录.join(format!("{}.md", 项目名));

    let mut 全文 = format!("# {} 项目文档\n\n", 项目名);
    全文.push_str(&标记段落);
    if let Err(e) = fs::write(&输出路径, &全文) {
        eprintln!("[智语言] 错误：写入文档失败: {}", e);
        return;
    }
    eprintln!(
        "[智语言] 项目文档已生成：{}（{} 个源文件，{} 个文档节点）",
        输出路径.display(),
        文件列表.len(),
        总文档.len()
    );
}

/// 递归收集目录下所有 .智 源文件（跳过隐藏目录）
fn 递归收集智文件(目录: &std::path::Path) -> Vec<PathBuf> {
    let mut 结果 = Vec::new();
    if let Ok(条目) = fs::read_dir(目录) {
        for 项 in 条目.flatten() {
            let 路径 = 项.path();
            if 路径.is_dir() {
                let 名 = 路径.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if 名.starts_with('.') {
                    continue;
                }
                结果.extend(递归收集智文件(&路径));
            } else if 路径.extension().map(|e| e == "智").unwrap_or(false) {
                结果.push(路径);
            }
        }
    }
    结果
}
