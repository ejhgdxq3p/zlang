// 文件: 编辑器服务/src/主程序.rs
// 职责: 智语言 LSP 编辑器服务 — 诊断、悬停、跳转、补全、格式化、大纲

// 中文标识符全局约定（与 `前端/src/库.rs`、`基础功能/src/库.rs` 等 crate 根一致）
#![allow(non_snake_case)]
// 同上：跳转/位置的完整 API 会被后续阶段与单元测试使用，此处不逐项标注
#![allow(dead_code)]

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::*;
use std::collections::HashMap;

// ★ 跳转方案（2026-10-08）第一阶段落地：
//   `位置映射` = 源码(行,字符列) ↔ LSP(行,UTF-16列) 的**唯一转换点**（封堵"中文列宽"错位）
//   `定义跳转` = 复用**共享前端词法分析器**的第一阶段定义跳转（返回真实文件 URI）
// ⚠ 必须用 `#[path]`：Rust 不允许**用非 ASCII 模块名去自动加载文件**（E0754），
//   本项目所有中文模块名都走这个写法（见 `源码/前端/src/库.rs`）。
#[path = "定义跳转.rs"]
mod 定义跳转;
#[path = "位置映射.rs"]
mod 位置映射;
// ★ 第一阶段·跨文件：`导入` 的成员下钻（`数学.计算面积` → `数学.智`）
//   复用**编译器自身的模块加载器**，不自己写路径/聚合规则（见该文件头注释）
#[path = "跨文件跳转.rs"]
mod 跨文件跳转;
// ★ 符号与引用：**真语法树** + **真词法词条**（替换旧文本扫描版 —— 旧版返回伪 URI `file:///`、
//   中文列按字节算、按子串误命中；详见该文件头注释）
#[path = "符号与引用.rs"]
mod 符号与引用;
// ★ 增量索引：**内容指纹键控**的词法/语法结果复用（跳转方案 §13.4 第 4 项）
//   一次请求会反复解析同一份源码（`块配对` 内部还要一遍），跨文件扫描更是逐文件解析。
#[path = "缓存.rs"]
mod 缓存;
// ★ 悬停：真语法树给出真实声明（种类/签名/声明位置），替换旧的 16 词关键词字典
#[path = "悬停.rs"]
mod 悬停;
// ★ 第二阶段：`自身.成员` / 实例成员 `对象.成员`（轻量类型推断）
#[path = "成员解析.rs"]
mod 成员解析;

fn main() {
    eprintln!("智语言 编辑器服务 启动中...");

    let (连接, 输入输出线程) = Connection::stdio();

    let 服务器能力 = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions::default()),
        definition_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })
    .unwrap();

    let _初始化参数 = 连接.initialize(服务器能力).unwrap();
    eprintln!("编辑器服务 初始化完成 — 支持: 诊断, 悬停, 补全, 跳转, 引用, 符号, 格式化");

    let mut 文档内容: HashMap<Url, String> = HashMap::new();

    for 消息 in &连接.receiver {
        match 消息 {
            Message::Request(请求) => {
                if 连接.handle_shutdown(&请求).unwrap() {
                    break;
                }
                let 响应 = 处理请求(&请求, &文档内容);
                连接.sender.send(Message::Response(响应)).unwrap();
            }
            Message::Notification(通知) => {
                处理通知(&通知, &mut 文档内容, &连接);
            }
            Message::Response(_) => {}
        }
    }

    输入输出线程.join().unwrap();
}

fn 处理请求(请求: &Request, 文档: &HashMap<Url, String>) -> Response {
    let 编号 = 请求.id.clone();
    match 请求.method.as_str() {
        "textDocument/formatting" => {
            let 参数: DocumentFormattingParams =
                serde_json::from_value(请求.params.clone()).unwrap();
            let 源码 = 文档.get(&参数.text_document.uri).cloned().unwrap_or_default();
            let 编辑列表 = 格式化代码(&源码);
            Response {
                id: 编号,
                result: Some(serde_json::to_value(编辑列表).unwrap()),
                error: None,
            }
        }

        "textDocument/hover" => {
            let 参数: HoverParams =
                serde_json::from_value(请求.params.clone()).unwrap();
            // ★ 2026-10-08 重接：改由 `悬停::悬停` 从**真语法树**给出真实声明
            //   （种类 / 签名 / 声明位置，支持跨文件模块成员），
            //   取代原先「16 个硬编码关键词 + 其余一律『标识符』」的空壳实现。
            let 文档地址 = 参数.text_document_position_params.text_document.uri.clone();
            let 源码 = 文档.get(&文档地址).cloned().unwrap_or_default();
            let 位置 = 参数.text_document_position_params.position;
            let 文件路径 = 文档地址.to_file_path().ok();
            let 内容 = 悬停::悬停(&源码, &文档地址, 文件路径.as_deref(), 位置);
            let 悬停内容 = match 内容 {
                Some((文本, 范围)) => Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: 文本,
                    }),
                    range: 范围,
                },
                None => Hover {
                    contents: HoverContents::Scalar(MarkedString::String(String::new())),
                    range: None,
                },
            };
            Response {
                id: 编号,
                result: Some(serde_json::to_value(悬停内容).unwrap()),
                error: None,
            }
        }

        "textDocument/definition" => {
            let 参数: GotoDefinitionParams =
                serde_json::from_value(请求.params.clone()).unwrap();
            // ★ 2026-10-08 重接：改为复用共享前端词法分析器的真实实现（方案 §8 第一阶段）
            //   旧实现（`查找定义` 文本扫描）返回**伪 URI `file:///def`** 且只认"以 `名字(` 开头的行"，
            //   现已废弃；宏 `跳转方案` 文档 §1.3.1 记录了旧行为的全部缺陷。
            let 文档地址 = 参数.text_document_position_params.text_document.uri.clone();
            let 源码 = 文档.get(&文档地址).cloned().unwrap_or_default();
            let 位置 = 参数.text_document_position_params.position;
            let 文件路径 = 文档地址.to_file_path().ok();
            let mut 位置列表 = 定义跳转::查找定义(&源码, &文档地址, 文件路径.as_deref(), 位置);
            if 位置列表.is_empty() {
                // ★ 同文件无结果 ⇒ 再试**跨文件**：`导入 数学` + `数学.计算面积` → `数学.智`
                //   （覆盖 普通导入 / 别名 / 相对路径 / 聚合展开 / 聚合别名 / 展开裸名）
                位置列表 = 跨文件跳转::查找定义(&源码, &文档地址, 文件路径.as_deref(), 位置);
            }
            if 位置列表.is_empty() {
                // ★ 第二阶段：`自身.成员` / 实例成员 `对象.成员`（轻量类型推断 + 跨文件类型定义）
                位置列表 = 成员解析::查找定义(&源码, &文档地址, 文件路径.as_deref(), 位置);
            }
            Response {
                id: 编号,
                result: Some(serde_json::to_value(GotoDefinitionResponse::Array(位置列表)).unwrap()),
                error: None,
            }
        }

        "textDocument/completion" => {
            let 参数: CompletionParams =
                serde_json::from_value(请求.params.clone()).unwrap();
            let 源码 = 文档
                .get(&参数.text_document_position.text_document.uri)
                .cloned()
                .unwrap_or_default();
            let 行号 = 参数.text_document_position.position.line as usize;
            let 列号 = 参数.text_document_position.position.character as usize;
            let 补全列表 = 提供补全(&源码, 行号, 列号);
            Response {
                id: 编号,
                result: Some(serde_json::to_value(补全列表).unwrap()),
                error: None,
            }
        }

        "textDocument/documentSymbol" => {
            let 参数: DocumentSymbolParams =
                serde_json::from_value(请求.params.clone()).unwrap();
            // ★ 2026-10-08 重接：改由 `符号与引用::文档符号` 从**真语法树**产出，
            //   URI 用文档真实地址（旧实现返回伪 URI `file:///` ⇒ 大纲点条目跳不动）
            let 文档地址 = 参数.text_document.uri.clone();
            let 源码 = 文档.get(&文档地址).cloned().unwrap_or_default();
            let 符号列表 = 符号与引用::文档符号(&源码);
            Response {
                id: 编号,
                result: Some(serde_json::to_value(符号列表).unwrap()),
                error: None,
            }
        }

        "textDocument/references" => {
            let 参数: ReferenceParams =
                serde_json::from_value(请求.params.clone()).unwrap();
            // ★ 2026-10-08 重接：`符号与引用::查找引用` 按**真词法词条**精确匹配
            //   （旧实现按 `str::find` 子串匹配 ⇒ `计算` 命中 `计算结果`；且把**字节偏移**当 UTF-16 列）
            let 文档地址 = 参数.text_document_position.text_document.uri.clone();
            let 源码 = 文档.get(&文档地址).cloned().unwrap_or_default();
            // ★ 传真实文件路径 ⇒ 启用**跨文件**引用扫描（项目源文件 + 模块可见性判定）
            let 文件路径 = 文档地址.to_file_path().ok();
            let 引用列表 = 符号与引用::查找引用(
                &源码,
                &文档地址,
                文件路径.as_deref(),
                参数.text_document_position.position,
                参数.context.include_declaration,
            );
            Response {
                id: 编号,
                result: Some(serde_json::to_value(引用列表).unwrap()),
                error: None,
            }
        }

        _ => Response {
            id: 编号,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: lsp_server::ErrorCode::MethodNotFound as i32,
                message: format!("不支持: {}", 请求.method),
                data: None,
            }),
        },
    }
}

fn 处理通知(
    通知: &Notification,
    文档: &mut HashMap<Url, String>,
    连接: &Connection,
) {
    match 通知.method.as_str() {
        "textDocument/didOpen" => {
            let 参数: DidOpenTextDocumentParams =
                serde_json::from_value(通知.params.clone()).unwrap();
            let 文档地址 = 参数.text_document.uri.clone();
            let 源码文本 = 参数.text_document.text.clone();
            文档.insert(文档地址.clone(), 源码文本.clone());
            发布诊断(&文档地址, &源码文本, 连接);
        }
        "textDocument/didChange" => {
            let 参数: DidChangeTextDocumentParams =
                serde_json::from_value(通知.params.clone()).unwrap();
            let 文档地址 = 参数.text_document.uri.clone();
            let 新文本 = 参数.content_changes.first()
                .map(|c| c.text.clone())
                .unwrap_or_default();
            文档.insert(文档地址.clone(), 新文本.clone());
            发布诊断(&文档地址, &新文本, 连接);
        }
        "textDocument/didClose" => {
            let 参数: DidCloseTextDocumentParams =
                serde_json::from_value(通知.params.clone()).unwrap();
            文档.remove(&参数.text_document.uri);
        }
        _ => {}
    }
}

// ============================================================
// 诊断 — 使用结构体 API（消除字符串解析反模式）
// ============================================================

fn 发布诊断(文档地址: &Url, 源码文本: &str, 连接: &Connection) {
    use frontend::诊断收集器::诊断收集器;

    let mut 诊断 = 诊断收集器::新建();
    let 文件路径 = 文档地址.to_string();
    // ★ 2026-10-08 修复（真 bug）：语义层解析 `导入` 时需要**文档的真实文件路径** ——
    //   它据此 `检测项目根` 并确定同级模块的搜索基准（`语义分析器.rs:873`）。
    //   此前从不调用 `设置入口文件路径` ⇒ 入口路径为 None ⇒ 搜索基准退化成**进程 cwd**、
    //   且同级目录不入搜索路径 ⇒ **任何含 `导入` 的文件在编辑器里都被假报「无法找到模块【X】」**
    //   （实测：加这一行后同一夹具诊断从 1 条错误变为 0 条）。
    //   ⚠ 必须用 `to_file_path()` 而不是 URI 字符串：`检测项目根` 走的是真实文件系统。
    let 真实路径 = 文档地址
        .to_file_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| 文件路径.clone());

    // 注册源文件以支持代码上下文显示
    诊断.注册源文件(&文件路径, 源码文本);

    let 词条列表 =
        frontend::词法分析::词法分析器::新建(源码文本, 文件路径, &mut 诊断).扫描所有词条();
    if 诊断.有错误() {
        发送诊断_结构体(文档地址, &诊断, 连接);
        return;
    }

    let mut 程序 = frontend::语法分析::语法分析器::新建(&词条列表, &mut 诊断).解析程序();
    if 诊断.有错误() {
        发送诊断_结构体(文档地址, &诊断, 连接);
        return;
    }
    let mut 语义 = frontend::语义::语义分析器::新建(&mut 诊断);
    语义.设置入口文件路径(&真实路径);
    let _ = 语义.分析程序(&mut 程序);
    发送诊断_结构体(文档地址, &诊断, 连接);
}

/// 使用结构体 API 发送诊断，消除字符串解析
fn 发送诊断_结构体(文档地址: &Url, 诊断: &frontend::诊断收集器::诊断收集器, 连接: &Connection) {
    let 诊断列表: Vec<Diagnostic> = 诊断.获取诊断列表()
        .iter()
        .map(|诊断项| {
            let 行 = 诊断项.位置.行号 as u32;
            let 列 = 诊断项.位置.列号 as u32;
            let 严重程度 = match 诊断项.级别 {
                frontend::公共数据结构::诊断级别::错误 => DiagnosticSeverity::ERROR,
                frontend::公共数据结构::诊断级别::警告 => DiagnosticSeverity::WARNING,
            };
            // 使用结构体字段构造消息，不再从字符串解析位置
            let 消息 = format!(
                "{}: {}: 【{}】{}",
                诊断项.级别文本(),
                诊断项.类型文本(),
                诊断项.描述,
                if let Some(帮助) = &诊断项.帮助信息 {
                    format!("\n帮助: {}", 帮助)
                } else {
                    String::new()
                }
            );
            Diagnostic {
                range: Range {
                    start: Position::new(行.saturating_sub(1), 列.saturating_sub(1)),
                    end: Position::new(行.saturating_sub(1), 列.saturating_sub(1).saturating_add(1).min(255)),
                },
                severity: Some(严重程度),
                message: 消息,
                ..Default::default()
            }
        })
        .collect();

    let 参数 = PublishDiagnosticsParams {
        uri: 文档地址.clone(),
        diagnostics: 诊断列表,
        version: None,
    };

    let 通知 = Notification {
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::to_value(参数).unwrap(),
    };

    连接.sender.send(Message::Notification(通知)).unwrap();
}

// ============================================================
// 悬停提示
// ============================================================
//
// ⚠ 2026-10-08：旧的 `分析悬停` / `分析标识符`（16 个硬编码关键词 + 其余一律「标识符」）
//   **已删除**，改由 `悬停` 模块从**真语法树**产出真实声明信息。旧实现的问题：
//     · 对**任何用户自定义符号**都没有信息量（函数签名/形参/返回类型/字段/枚举成员/声明文件全无）；
//     · 表里把 `整数`/`小数`/`文本` 说成"内置**函数**"——它们是**类型关键字**（同一份表里自相矛盾）；
//     · `输出`/`启动程序` 是**用户代码里的标识符**（内置函数、程序入口），却与真正的关键字混在一张表里，
//       一旦用户在别处定义同名符号（如 `常量 输出 = 1`）就会给出**错误**说明；
//     · 悬停内容固定 `HoverContents::Scalar`（纯文本），无法呈现签名排版。
//   新实现：`HoverContents::Markup` + Markdown，且**用户符号一律走 AST**，关键字表只保留真正的关键字。

// ⚠ 旧的 `提取光标单词`（直接用 `chars()[LSP列]` 取值）**已删除**：UTF-16 列 ≠ 字符下标，
//   中文行会整体错位。新版见文件下方「光标处单词提取」一节（供"查找引用/全角标点"等复用）。

// ============================================================
// 跳转到定义
// ============================================================
//
// ⚠ 2026-10-08：旧的 `查找定义(源码, 行号, 列号)` **已删除**（文本扫描版）。
//   它只做「取光标处单词 → 全文找 `以 单词( 开头的行`」，缺陷见《跳转方案》§1.3.1：
//     · 仅能命中**同文件、行首的普通函数**；变量/常量/结构体/枚举/接口/字段/方法/
//       `自身.`/实例./模块/导入/类型标注/运算符重载 全部不支持；
//     · `starts_with` **前缀误命中**（`加法(` 会命中 `加法运算(`）；
//     · 注释与字符串里的 `名字(` 被当成定义；
//     · 返回**伪 URI `file:///def`** ⇒ 即使范围算对也无法跳转。
//   现由 `定义跳转::查找定义` 取代（复用共享前端词法分析器 + `位置映射` 的正确列宽）。

// ============================================================
// 光标处单词提取（供 悬停 / 引用 复用）
// ============================================================
/// ⚠ `列号` 是 LSP 的 **UTF-16 code unit 列**，必须先转成**字符下标**再切行，
/// 否则中文行会整体错位（改造前直接用 `chars()[列号]` 取值）。
fn 提取光标单词(源码: &str, 行号: usize, 列号: usize) -> String {
    let Some(行文本) = 源码.lines().nth(行号) else {
        return String::new();
    };
    let 字符: Vec<char> = 行文本.chars().collect();
    if 字符.is_empty() {
        return String::new();
    }

    // UTF-16 列 ⇒ 字符下标（落在代理对中间时向下取整；越界落到行尾）
    let mut 光标 = 字符.len();
    let mut 累计 = 0usize;
    for (下标, c) in 字符.iter().enumerate() {
        if 累计 >= 列号 {
            光标 = 下标;
            break;
        }
        累计 += c.len_utf16();
    }

    let 是词字符 = |c: char| c.is_alphanumeric() || c == '_';
    if 光标 >= 字符.len() || !是词字符(字符[光标]) {
        // VS Code 常在"词尾"触发请求 ⇒ 允许向左退一格
        if 光标 == 0 || !是词字符(字符[光标 - 1]) {
            return String::new();
        }
        光标 -= 1;
    }

    let mut 开始 = 光标;
    while 开始 > 0 && 是词字符(字符[开始 - 1]) {
        开始 -= 1;
    }
    let mut 结束 = 光标;
    while 结束 < 字符.len() && 是词字符(字符[结束]) {
        结束 += 1;
    }
    字符[开始..结束].iter().collect()
}

// ============================================================
// 代码补全
// ============================================================

fn 提供补全(_源码: &str, _行号: usize, _列号: usize) -> CompletionResponse {
    let 补全项列表 = vec![
        关键字补全("如果", "如果 条件:\n    内容\n结束", "流程控制 — 条件分支"),
        关键字补全("否则如果", "否则如果 条件:\n    内容", "流程控制 — 多条件分支"),
        关键字补全("否则", "否则:\n    内容", "流程控制 — 否则分支"),
        关键字补全("循环", "循环:\n    内容\n结束", "流程控制 — 无限循环"),
        关键字补全("遍历", "遍历 元素 在 容器:\n    内容\n结束", "流程控制 — 遍历"),
        // ⚠ 订正（2026-09-20，按规范第 3265-3273 行「函数定义中"函数"关键字的省略」）：
        //   `函数` 是**保留关键字**（规范第 721 行「必须省略」）——
        //   命名函数定义必须写成 `打招呼(): … 结束`，写 `函数 名称():` 会被解析器报错
        //   （`语法分析器.rs` 第 1231-1242 行：「函数定义不能写 函数 关键字，直接写 函数名():」），
        //   `函数` 仅用于**匿名函数** `函数(参数): … 结束` 与**函数类型标注**。
        //   原补全项插入 `函数 名称(参数):` ⇒ **补全出来的代码语法非法**。
        //   ⇒ 改为插入 `函数` 唯一合法的匿名函数骨架（与其余补全项「插入文本以触发关键字开头」的约定一致）。
        关键字补全("函数", "函数(参数):\n    内容\n结束", "匿名函数（命名函数直接写 函数名()，禁止写 函数 关键字）"),
        关键字补全("结构体", "结构体 名称:\n    字段\n结束", "结构体定义"),
        关键字补全("返回", "返回", "从函数返回值"),
        关键字补全("导入", "导入 模块名", "导入模块"),
        关键字补全("输出", "输出(内容)", "向标准输出打印"),
        关键字补全("启动程序", "启动程序():\n    内容\n结束", "程序入口"),
    ];

    CompletionResponse::Array(
        补全项列表
            .into_iter()
            .map(|项| CompletionItem {
                label: 项.标签,
                detail: Some(项.详述),
                insert_text: Some(项.插入文本),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            })
            .collect(),
    )
}

struct 补全项 { 标签: String, 插入文本: String, 详述: String }
fn 关键字补全(标签: &str, 插入文本: &str, 详述: &str) -> 补全项 {
    补全项 { 标签: 标签.to_string(), 插入文本: 插入文本.to_string(), 详述: 详述.to_string() }
}

// ============================================================
// 代码格式化
// ============================================================

fn 格式化代码(源码: &str) -> Vec<TextEdit> {
    let mut 编辑列表 = Vec::new();
    let mut 格式化行 = Vec::new();
    let mut 缩进级别: usize = 0;
    let 缩进宽度: usize = 4;

    for 行 in 源码.lines() {
        let 修剪行 = 行.trim();
        if 修剪行.is_empty() {
            格式化行.push(String::new());
            continue;
        }

        // 结束减少缩进
        if 修剪行 == "结束" {
            缩进级别 = 缩进级别.saturating_sub(1);
        }

        // 应用缩进
        let 缩进 = " ".repeat(缩进级别 * 缩进宽度);
        格式化行.push(format!("{}{}", 缩进, 修剪行));

        // 冒号结尾增加缩进
        if 修剪行.ends_with(':') {
            缩进级别 += 1;
        }

        // 结束后的立即恢复
        if 修剪行 == "结束" {
            // 保持当前缩进
        }
    }

    let 新文本 = 格式化行.join("\n");
    if 新文本 != 源码 {
        let 总行数 = 源码.lines().count() as u32;
        编辑列表.push(TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(总行数, 0),
            },
            new_text: 新文本,
        });
    }

    编辑列表
}

// ============================================================
// 文档符号（大纲/Breadcrumb）· 查找引用
// ============================================================
//
// ⚠ 2026-10-08：旧的 `分析符号` / `提取名称` / `查找引用`（文本扫描版）**已删除**，
//   改由 `符号与引用` 模块基于**共享前端**实现。旧实现的缺陷（都有据可查，不是"想换就换"）：
//     · `documentSymbol` 返回**伪 URI `file:///`** ⇒ 大纲点条目无法跳转；
//       `range.end = 行.len()` 把 **UTF-8 字节数**当 UTF-16 列 ⇒ 含中文的行范围整体超长
//       （如 `结构体 学生:` 字节 13 / 字符 7）；
//       `starts_with("函数 ")` **永不命中**（规范禁止写 `函数 名称()` —— 见本文件补全表里
//       早就记录的同一条规则）⇒ **所有函数都漏报**；
//       结构体字段/方法、枚举成员、接口方法、变量、常量、导入 **全部不报**；且不认 `私有`。
//     · `references` 同样是伪 URI；把 `str::find` 的**字节偏移**当 UTF-16 列 ⇒ 中文行位置全错；
//       按**子串**匹配 ⇒ `计算` 会命中 `计算结果`；在**注释与字符串**内也匹配 ⇒ 假引用。

