// 文件: 版本管理器/src/命令/生成关联.rs
// 职责: 「生成关联」命令 — 从 C 头文件/库目录生成智语言 FFI 绑定模板
// 依据: 智语言编程规范 §6.5.2（外部函数接口的导入与使用）+ §6.5.9（.h 类型解析字典）
//
// 命令格式（规范 §6.5.2「库与头文件自动解析」段）:
//   智语言 生成关联 <路径> <别名> [输出目录] [--过滤 "模式"]
//
// 自动区分规则:
//   - 路径以 .h 结尾 → 单个头文件模式，生成 `<别名>.绑定.智`
//   - 路径为目录     → 库模式，扫描所有 .h 生成 `<别名>.智`
//
// 生成文件语法 = 规范 §6.5.2 新语法:
//   外部:
//       导入 <代号>                       # 代号 = 别名
//       <中文别名> 关联 <代号>.<C符号>    # 独立关联行（无顺序要求）
//       <中文函数名>(参数列表) 返回类型:
//       结束
//   结束
//   宏常量: <中文名> 关联 <代号>.<C符号> + 常量 <中文名> = <值>

use std::path::Path;
use std::fs;

/// C 函数声明提取结果
struct C函数 {
    名称: String,
    返回类型: String,
    形参列表: Vec<(String, String)>,  // (类型, 名称)
}

/// C 结构体提取结果
struct C结构体 {
    名称: String,
    字段列表: Vec<(String, String)>,  // (名称, 类型)
}

/// C 宏常量提取结果
struct C宏常量 {
    名称: String,
    值: String,
}

pub fn 执行(参数: &[String]) {
    if 参数.is_empty() {
        显示帮助();
        return;
    }
    // 解析: <路径> <别名> [输出目录] [--过滤 "模式"]
    let mut 位置参数 = Vec::new();
    let mut 过滤模式: Option<String> = None;
    let mut i = 0;
    while i < 参数.len() {
        let 词 = &参数[i];
        if 词 == "--过滤" {
            if i + 1 < 参数.len() {
                过滤模式 = Some(参数[i + 1].clone());
                i += 2;
                continue;
            } else {
                eprintln!("[智语言] 错误：--过滤 需要后续模式参数");
                return;
            }
        }
        位置参数.push(词.clone());
        i += 1;
    }

    if 位置参数.len() < 2 {
        eprintln!("[智语言] 错误：需要 <路径> 和 <别名> 参数");
        显示帮助();
        return;
    }
    let 路径 = 位置参数[0].clone();
    let 别名 = 位置参数[1].clone();
    let 输出目录 = 位置参数.get(2).cloned().unwrap_or_else(|| ".".to_string());

    // 输出目录不存在则自动创建（规范未要求，但避免 os error 3 直接失败）
    if let Err(e) = fs::create_dir_all(&输出目录) {
        eprintln!("[智语言] 错误：无法创建输出目录 '{}': {}", 输出目录, e);
        return;
    }

    let 路径对象 = Path::new(&路径);
    if 路径对象.is_dir() {
        处理库(&路径, &别名, &输出目录, &过滤模式);
    } else if 路径对象.is_file() && (路径.ends_with(".h") || 路径.ends_with(".hpp")) {
        处理头文件(&路径, &别名, &输出目录);
    } else {
        eprintln!("[智语言] 错误：路径不存在或不是 .h 头文件/目录: {}", 路径);
        显示帮助();
    }
}

fn 显示帮助() {
    eprintln!("智语言 生成关联 — 从 C 头文件/库目录生成智语言 FFI 绑定模板");
    eprintln!();
    eprintln!("用法:");
    eprintln!("  智语言 生成关联 <路径> <别名> [输出目录] [--过滤 \"模式\"]");
    eprintln!("    <路径> 为 .h 文件 → 单头文件模式，生成 <别名>.绑定.智");
    eprintln!("    <路径> 为目录       → 库模式，扫描全部 .h 生成 <别名>.智");
    eprintln!("    [输出目录]          可选，默认当前目录");
    eprintln!("    --过滤 \"模式\"       可选，仅库模式生效，仅解析文件名含模式的头文件");
}

fn 处理头文件(头文件路径: &str, 别名: &str, 输出目录: &str) {
    if !Path::new(头文件路径).exists() {
        eprintln!("[智语言] 错误：头文件不存在: {}", 头文件路径);
        return;
    }
    let 内容 = match fs::read_to_string(头文件路径) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[智语言] 错误：无法读取头文件 '{}': {}", 头文件路径, e);
            return;
        }
    };

    let 函数列表 = 提取C函数(&内容);
    let 结构体列表 = 提取C结构体(&内容);
    let 宏列表 = 提取C宏常量(&内容);

    // 单头文件模式：生成 <别名>.绑定.智（待用户将英文别名替换为中文）
    let 绑定内容 = 生成绑定模板(别名, &函数列表, &结构体列表, &宏列表, true);
    let 输出路径 = format!("{}/{}.绑定.智", 输出目录, 别名);
    match fs::write(&输出路径, &绑定内容) {
        Ok(_) => eprintln!("[智语言] 已生成绑定文件: {}", 输出路径),
        Err(e) => eprintln!("[智语言] 错误：写入文件失败 '{}': {}", 输出路径, e),
    }
}

fn 处理库(库目录: &str, 别名: &str, 输出目录: &str, 过滤模式: &Option<String>) {
    if !Path::new(库目录).is_dir() {
        eprintln!("[智语言] 错误：库目录不存在: {}", 库目录);
        return;
    }

    let mut 所有函数 = Vec::new();
    let mut 所有结构体 = Vec::new();
    let mut 所有宏 = Vec::new();

    let 条目列表 = match fs::read_dir(库目录) {
        Ok(条目) => 条目,
        Err(e) => {
            eprintln!("[智语言] 错误：无法读取库目录 '{}': {}", 库目录, e);
            return;
        }
    };

    for 条目 in 条目列表 {
        if let Ok(条目) = 条目 {
            let 路径 = 条目.path();
            if let Some(扩展) = 路径.extension() {
                if 扩展 == "h" || 扩展 == "hpp" {
                    // 库模式 --过滤 仅解析文件名含模式的头文件
                    if let Some(模式) = 过滤模式 {
                        let 名 = 路径.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        if !名.contains(模式.as_str()) {
                            continue;
                        }
                    }
                    if let Ok(内容) = fs::read_to_string(&路径) {
                        所有函数.append(&mut 提取C函数(&内容));
                        所有结构体.append(&mut 提取C结构体(&内容));
                        所有宏.append(&mut 提取C宏常量(&内容));
                    }
                }
            }
        }
    }

    // 库模式：生成 <别名>.智（可直接导入使用的完整模块）
    let 绑定内容 = 生成绑定模板(别名, &所有函数, &所有结构体, &所有宏, false);
    let 输出路径 = format!("{}/{}.智", 输出目录, 别名);
    match fs::write(&输出路径, &绑定内容) {
        Ok(_) => eprintln!("[智语言] 已生成库模块文件: {} ({} 个函数, {} 个宏)", 输出路径, 所有函数.len(), 所有宏.len()),
        Err(e) => eprintln!("[智语言] 错误：写入文件失败 '{}': {}", 输出路径, e),
    }
}

/// 从 C 头文件中提取函数声明
fn 提取C函数(内容: &str) -> Vec<C函数> {
    let mut 函数列表 = Vec::new();
    for 行 in 内容.lines() {
        let 行 = 行.trim();
        if 行.starts_with("//") || 行.starts_with("/*") || 行.starts_with("*")
            || 行.starts_with("#") || 行.starts_with("typedef")
            || 行.is_empty() {
            continue;
        }
        if let Some(函数) = 匹配C函数声明(行) {
            函数列表.push(函数);
        }
    }
    函数列表
}

/// 尝试匹配一行 C 函数声明
fn 匹配C函数声明(行: &str) -> Option<C函数> {
    let 左括号位置 = 行.find('(')?;
    let 右括号位置 = 行.rfind(')')?;

    let 签名部分 = 行[..左括号位置].trim();
    let 最后空格 = 签名部分.rfind(char::is_whitespace)?;

    let 返回类型 = 签名部分[..最后空格].trim().to_string();
    let 函数名 = 签名部分[最后空格..].trim()
        .trim_end_matches('*').trim().to_string();

    let 参数部分 = 行[左括号位置+1..右括号位置].trim();
    let mut 形参列表 = Vec::new();
    if !参数部分.is_empty() && 参数部分 != "void" {
        for 参数声明 in 参数部分.split(',') {
            let 参数声明 = 参数声明.trim();
            if 参数声明 == "..." || 参数声明 == "void" {
                continue;
            }
            if let Some(空格位置) = 参数声明.rfind(char::is_whitespace) {
                let 类型 = 参数声明[..空格位置].trim().to_string();
                let 名称 = 参数声明[空格位置..].trim()
                    .trim_end_matches('*').trim().to_string();
                if !名称.is_empty() && !类型.is_empty() {
                    形参列表.push((类型, 名称));
                }
            } else {
                形参列表.push((参数声明.to_string(), String::new()));
            }
        }
    }

    Some(C函数 {
        名称: 函数名,
        返回类型,
        形参列表,
    })
}

/// 从 C 头文件中提取结构体定义
fn 提取C结构体(内容: &str) -> Vec<C结构体> {
    let mut 结构体列表 = Vec::new();
    let 行集合: Vec<&str> = 内容.lines().collect();
    let mut i = 0;
    while i < 行集合.len() {
        let 行 = 行集合[i].trim();
        if 行.starts_with("struct ") || 行.starts_with("typedef struct") {
            let mut 名称 = String::new();
            if 行.starts_with("typedef struct") {
                let 剩余 = 行[14..].trim_start();
                if 剩余.starts_with('{') {
                    名称 = String::new();
                } else if let Some(括号位置) = 剩余.find('{') {
                    名称 = 剩余[..括号位置].trim().to_string();
                }
            } else {
                let 剩余 = 行[6..].trim_start();
                if 剩余.starts_with('{') {
                    名称 = String::new();
                } else if let Some(括号位置) = 剩余.find('{') {
                    名称 = 剩余[..括号位置].trim().to_string();
                } else {
                    名称 = 剩余.to_string();
                }
            }

            if !行.contains('{') {
                if 行.trim_end().ends_with(';') {
                    i += 1;
                    continue;
                }
                i += 1;
                while i < 行集合.len() {
                    if 行集合[i].trim().contains('{') {
                        break;
                    }
                    i += 1;
                }
                if i >= 行集合.len() { break; }
            }

            let mut 字段列表 = Vec::new();
            i += 1;
            while i < 行集合.len() {
                let 字段行 = 行集合[i].trim();
                if 字段行.starts_with('}') {
                    if 名称.is_empty() && 字段行.len() > 1 {
                        let 别名部分 = 字段行[1..].trim().trim_end_matches(';').trim();
                        if !别名部分.is_empty() {
                            名称 = 别名部分.to_string();
                        }
                    }
                    break;
                }
                if 字段行.starts_with("//") || 字段行.starts_with("/*")
                    || 字段行.starts_with('*') || 字段行.starts_with('#')
                    || 字段行.is_empty() {
                    i += 1;
                    continue;
                }
                if let Some(分号位置) = 字段行.find(';') {
                    let 字段声明 = 字段行[..分号位置].trim();
                    if let Some(最后的空格) = 字段声明.rfind(char::is_whitespace) {
                        let 类型 = 字段声明[..最后的空格].trim().to_string();
                        let 字段名 = 字段声明[最后的空格..].trim()
                            .trim_end_matches('*').trim()
                            .trim_end_matches('[').to_string();
                        if !字段名.is_empty() && !类型.is_empty() {
                            字段列表.push((字段名, 类型));
                        }
                    }
                }
                i += 1;
            }

            if !名称.is_empty() || !字段列表.is_empty() {
                结构体列表.push(C结构体 {
                    名称: if 名称.is_empty() { "未命名结构体".to_string() } else { 名称 },
                    字段列表,
                });
            }
        }
        i += 1;
    }
    结构体列表
}

/// 从 C 头文件中提取宏常量
fn 提取C宏常量(内容: &str) -> Vec<C宏常量> {
    let mut 宏列表 = Vec::new();
    for 行 in 内容.lines() {
        let 行 = 行.trim();
        if 行.starts_with("#define") {
            let 剩余 = 行[7..].trim();
            if let Some(第一个空格) = 剩余.find(char::is_whitespace) {
                let 宏名 = 剩余[..第一个空格].trim().to_string();
                let 值 = 剩余[第一个空格..].trim().to_string();
                宏列表.push(C宏常量 { 名称: 宏名, 值 });
            }
        }
    }
    宏列表
}

/// 生成智语言绑定模板文件
///
/// `单头文件` = true → 生成 `<别名>.绑定.智`（待用户编辑中文别名）；
/// `单头文件` = false → 生成 `<别名>.智`（库模式，可直接导入使用）。
///
/// 代号 = 别名（规范示例 `生成关联 ... 数据库` 后 `导入 数据库`）。
/// 关联行格式：`中文别名 关联 <代号>.<C符号>`（独立声明语句，无顺序要求）。
/// 宏常量：`常量 <中文名> = <值>`（不带关联，规范 §6.5.2 第 6 例）。
fn 生成绑定模板(别名: &str, 函数列表: &[C函数], 结构体列表: &[C结构体], 宏列表: &[C宏常量], 单头文件: bool) -> String {
    let mut 输出 = String::new();

    if 单头文件 {
        输出.push_str(&format!("// 自动生成的绑定文件（单头文件模式）\n"));
        输出.push_str(&format!("// 别名：{}\n", 别名));
        输出.push_str("// 请将英文别名替换为中文后再导入使用\n\n");
    } else {
        输出.push_str(&format!("// 自动生成的绑定模块（库模式）\n"));
        输出.push_str(&format!("// 别名：{}\n\n", 别名));
    }

    输出.push_str("外部:\n");
    // 块级导入代号（规范 §6.5.2 语法）
    输出.push_str(&format!("    导入 {}\n\n", 别名));

    // 函数映射：独立关联行 + 函数签名（智语言类型）
    for 函数 in 函数列表 {
        if 单头文件 {
            输出.push_str(&format!("    // TODO: 将「{}」改为中文函数名\n", 函数.名称));
        }
        // 关联行：中文别名 关联 代号.C符号（单头文件用英文名占位，库模式直接用原名）
        输出.push_str(&format!("    {} 关联 {}.{}\n", 函数.名称, 别名, 函数.名称));

        // 函数签名（无函数体）
        输出.push_str(&函数.名称);
        输出.push('(');
        let 形参声明: Vec<String> = 函数.形参列表.iter()
            .map(|(类型, 名称)| {
                let 智类型 = c类型到智类型(类型);
                format!("{} {}", 智类型, if 名称.is_empty() { "参数" } else { 名称 })
            })
            .collect();
        输出.push_str(&形参声明.join(", "));
        输出.push(')');

        let 返回 = c类型到智类型(&函数.返回类型);
        if 返回 != "空值" {
            输出.push_str(&format!(" {}:", 返回));
        } else {
            输出.push(':');
        }
        输出.push_str("\n结束\n\n");
    }

    // 宏常量（规范 §6.5.2 第 6 例：成功码 关联 数据库.SQLITE_OK + 常量 成功码 = 0）
    // 关联行 + 常量声明（关联行独立成行，无顺序要求；C 符号名取宏原名）
    for 宏 in 宏列表 {
        if 单头文件 {
            输出.push_str(&format!("    // TODO: 将「{}」改为中文常量名\n", 宏.名称));
        }
        输出.push_str(&format!("    {} 关联 {}.{}\n", 宏.名称, 别名, 宏.名称));
        输出.push_str(&format!("    常量 {} = {}\n\n", 宏.名称, 宏.值));
    }

    // 结构体定义（规范 §6.5.2 第 7 例语法：结构体 名(): 字段: 类型 = 默认值 结束）
    for 结构体 in 结构体列表 {
        输出.push_str(&format!("    结构体 {}():\n", 结构体.名称));
        for (字段名, 字段类型) in &结构体.字段列表 {
            let 智类型 = c类型到智类型(字段类型);
            输出.push_str(&format!("        {}: {} = {}\n", 字段名, 智类型,
                类型默认值(&智类型)));
        }
        输出.push_str("    结束\n\n");
    }

    输出.push_str("结束\n");

    // 使用示例注释
    输出.push_str("\n// === 使用示例 ===\n");
    输出.push_str(&format!("// 导入 {}\n", 别名));
    输出.push_str("//\n");
    输出.push_str("// 启动程序():\n");
    输出.push_str("//     输出(\"绑定就绪\")\n");
    输出.push_str("// 结束\n");

    输出
}

/// 根据精确 FFI 类型名返回字段默认值字面量
fn 类型默认值(智类型: &str) -> &'static str {
    match 智类型 {
        "文本" => "\"\"",
        "逻辑" => "假",
        "小数" | "短小数" | "长小数" => "0.0",
        _ => "0",   // 整数类（单/微/短/长/平台/位宽 及 无符号系列）→ 0
    }
}

/// C 类型 → 智语言 FFI 精确类型名（规范 §6.5.9 字典，2026-08-29 命名改版）
///
/// 输出 FFI 精确类型名（单整数/短整数/长小数/无符号位宽整数 等），由 语义分析器
/// 在外部块语境识别为精确 C 类型码（公共降级::模块到FFI项 → shim 按声明位宽编组）。
/// 平台相关类型（long/size_t/ssize_t）映射为 平台整数/无符号位宽整数/位宽整数，
/// shim 声明端直接映射 Rust std::os::raw（c_long/c_size_t/c_ssize_t），平台差异委托 Rust。
///
/// 指针处理：char* → 文本（只读 C 字符串）；void*/int*/size_t* 等 → 指针
/// （不透明句柄，如 sqlite3*/CURL*；智语言无指针类型，外部地址统一用 `指针` 声明，
/// 调用层由 整数 承载地址值，编译器在 shim 层按 ABI 等价传递，对用户透明；
/// C 输出参数需用户按 `输出 指针 名` 手动标注）。
fn c类型到智类型(c类型: &str) -> &str {
    let 原 = c类型.trim();
    let 归一: String = 原.replace(' ', "").replace('\t', "");
    let 无前缀 = 归一.strip_prefix("const").unwrap_or(&归一);
    if 无前缀.ends_with('*') {
        let 基础 = 无前缀.trim_end_matches('*');
        return match 基础 {
            "char" => "文本",
            _ => "指针",
        };
    }
    match 无前缀 {
        "void" => "空值",
        "int" | "signed" | "signedint" => "短整数",
        "short" | "shortint" => "微整数",
        "long" | "longint" => "平台整数",
        "longlong" | "longlongint" => "长整数",
        "unsigned" | "unsignedint" => "无符号短整数",
        "unsignedshort" => "无符号微整数",
        "unsignedlong" | "unsignedlongint" => "无符号平台整数",
        "unsignedlonglong" => "无符号长整数",
        "char" => "字符",
        "signedchar" => "单整数",
        "unsignedchar" => "无符号单整数",
        "int8_t" => "单整数",
        "int16_t" => "微整数",
        "int32_t" => "短整数",
        "int64_t" => "长整数",
        "uint8_t" => "无符号单整数",
        "uint16_t" => "无符号微整数",
        "uint32_t" => "无符号短整数",
        "uint64_t" => "无符号长整数",
        "float" => "短小数",
        "double" => "长小数",
        "size_t" => "无符号位宽整数",
        "ssize_t" => "位宽整数",
        "bool" | "_Bool" => "逻辑",
        _ => "长整数",
    }
}
