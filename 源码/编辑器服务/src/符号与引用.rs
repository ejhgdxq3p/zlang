// 文件: 编辑器服务/src/符号与引用.rs
// 职责: 文档符号（大纲 / 面包屑）与查找所有引用 —— **复用共享前端的词法/语法**，返回**真实 URI 与正确列宽**
//
// ★ 为什么必须替换旧实现（`主程序.rs` 里的 `分析符号` / `查找引用` 文本扫描版）：
//   旧 `documentSymbol`：
//     ① 返回 **伪 URI `file:///`** ⇒ 大纲里点条目**无法跳转**（与旧 `definition` 同一类硬 bug）；
//     ② `range.end = 行.len()` —— Rust 的 `str::len()` 是**字节数**，被当成 UTF-16 列 ⇒
//        **凡含中文的行，范围整体超长**（"结构体 学生:" 字节 13 / 字符 7）；
//     ③ `starts_with("函数 ")` 分支**永不命中** —— 规范禁止写 `函数 名称()`（`函数` 只用于匿名函数），
//        故**全部函数都漏报**（本模块同期还发现：`主程序.rs` 的补全注释早已记录该规则）；
//     ④ 结构体字段 / 方法 / 枚举成员 / 接口方法 / 变量 / 常量 / 导入 **全部不报**；
//     ⑤ 不认 `私有` 前缀。
//   旧 `references`：
//     ① 伪 URI `file:///`；② `字符串.find` 返回**字节偏移**当 UTF-16 列 ⇒ 中文行位置全错；
//     ③ 按**子串**匹配 ⇒ `计算` 会命中 `计算结果`（假引用）；
//     ④ 在**注释与字符串**里也匹配 ⇒ 假引用。
//
// 本模块一律：
//   · URI 取自**文档真实 URI**（`Url` 由调用方传入，绝不 `Url::parse("file:///")`）；
//   · 行列转换**只经 `位置映射::行索引`**（唯一转换点，UTF-16 列由它算）；
//   · 符号来自**真语法树**（`frontend::语法分析::程序.顶层定义列表`），不是逐行 `starts_with`；
//   · 引用来自**真词法词条**（注释/字符串天然不是标识符词条），且**不做子串匹配**。
//
// ⚠ 本阶段已知口径（**不假装更强**）：
//   · `references` 只搜**当前文档**（跨文件引用需模块加载 + 语义消解，属第二阶段）；
//   · 同名的裸名与 `某对象.X` 成员**按"访问形态"区分**（裸名只匹配裸名；成员只匹配 `同一 base` 的成员），
//     但**不区分同名局部变量与同名模块级变量**（需作用域/语义分析）—— 这是纯词法方案的边界，如实标注。

use crate::位置映射::行索引;
use frontend::语法分析::{程序, 导出块的项, 外部块的项, 结构体成员, 顶层定义};
use frontend::词法分析::词条类型;
use lsp_types::{
    DocumentSymbol, DocumentSymbolResponse, Location, Position, Range, SymbolKind, Url,
};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

// ============================================================
// 文档符号（大纲 / 面包屑）
// ============================================================
/// ⚠ 与旧实现的关键差别：**返回 `DocumentSymbolResponse::Nested`**（带层级），
///   结构体字段/方法、枚举成员、接口方法挂在各自类型下面（旧实现是 Flat 且只有 4 类）。
///
/// ★ 关于"伪 URI `file:///`"这个旧 bug：改用 `DocumentSymbol` 后它**从结构上不可能复现** ——
///   `DocumentSymbolResponse::Nested` 的条目类型 `DocumentSymbol` **没有 uri 字段**
///   （LSP 规定其 range 一律位于本次请求的 textDocument 内）；旧 bug 的根因正是选错了响应类型
///   （用了带 uri 的 `SymbolInformation`/Flat，又拿 `Url::parse("file:///")` 硬凑一个）。
///   故本函数**不需要** URI 参数 —— 不是"漏了"，而是修法更彻底。
pub fn 文档符号(源码: &str) -> DocumentSymbolResponse {
    let 索引 = 行索引::新建(源码);
    let 配对 = crate::定义跳转::块配对(源码);
    // 词法只做一次：既喂给语法分析，也用来**精确定位名字列**（见 `名字范围`）
    let 词条列表 = 词法分析(源码);
    if 词条列表.is_empty() {
        return DocumentSymbolResponse::Nested(Vec::new());
    }
    let 程序 = 解析(源码);
    let 上下文 = 符号上下文 { 索引: &索引, 配对: &配对, 词条列表: &词条列表 };

    let mut 结果: Vec<DocumentSymbol> = Vec::new();

    for 导入 in &程序.导入列表 {
        // 大纲里显示**代码中真正可用**的名字：别名优先，否则**模块路径末段**
        // （规范 §2.7：命名空间 = 末段，`导入 核心库/数学` 在代码里写 `数学.成员` ⇒ 大纲也该显示 `数学`）。
        // 完整模块路径放进 `detail`：既不丢信息，也不让大纲出现 `核心库/数值常量`、`./工具` 这种"写不出来的名字"。
        let 完整路径 = 导入.模块路径.join("/");
        let 名 = 导入
            .别名
            .clone()
            .or_else(|| 导入.模块路径.last().cloned())
            .unwrap_or_else(|| 完整路径.clone());
        if 名.is_empty() {
            continue;
        }
        结果.push(上下文.造模块(&名, &完整路径, &导入.位置));
    }

    for 定义 in &程序.顶层定义列表 {
        match 定义 {
            顶层定义::函数定义(f) => {
                结果.push(上下文.造(&f.名称, SymbolKind::FUNCTION, &f.位置, Vec::new()));
            }
            顶层定义::变量声明(v) => {
                结果.push(上下文.造(&v.名称, SymbolKind::VARIABLE, &v.位置, Vec::new()));
            }
            顶层定义::常量声明(c) => {
                结果.push(上下文.造(&c.名称, SymbolKind::CONSTANT, &c.位置, Vec::new()));
            }
            顶层定义::结构体定义(s) => {
                let mut 子项 = Vec::new();
                // **构造参数**（`结构体 学生(文本 姓名, 整数 年龄)`）也是可见成员
                // （`定义跳转`/`成员解析`/`悬停` 都把它们当成员 ⇒ 大纲必须同口径，
                //  否则大纲里"看不到"一个明明能跳转、能悬停的符号）
                for p in &s.形参列表 {
                    子项.push(上下文.造(&p.名称, SymbolKind::VARIABLE, &p.位置, Vec::new()));
                }
                for 成员 in &s.成员列表 {
                    match 成员 {
                        结构体成员::字段 { 名称, 位置, .. } => {
                            子项.push(上下文.造(名称, SymbolKind::FIELD, 位置, Vec::new()));
                        }
                        结构体成员::方法(f) | 结构体成员::初始化(f) => {
                            子项.push(上下文.造(&f.名称, SymbolKind::METHOD, &f.位置, Vec::new()));
                        }
                    }
                }
                结果.push(上下文.造(&s.名称, SymbolKind::STRUCT, &s.位置, 子项));
            }
            顶层定义::枚举定义(e) => {
                let 子项 = e
                    .成员列表
                    .iter()
                    .map(|m| 上下文.造(&m.名称, SymbolKind::ENUM_MEMBER, &m.位置, Vec::new()))
                    .collect();
                结果.push(上下文.造(&e.名称, SymbolKind::ENUM, &e.位置, 子项));
            }
            顶层定义::接口定义(i) => {
                let 子项 = i
                    .方法声明列表
                    .iter()
                    .map(|m| 上下文.造(&m.名称, SymbolKind::METHOD, &m.位置, Vec::new()))
                    .collect();
                结果.push(上下文.造(&i.名称, SymbolKind::INTERFACE, &i.位置, 子项));
            }
            顶层定义::外部块(块) => {
                for 项 in &块.项列表 {
                    match 项 {
                        外部块的项::外部函数定义(f) => {
                            let 名 = 带前缀(&f.命名空间前缀, &f.名称);
                            结果.push(上下文.造(&名, SymbolKind::FUNCTION, &f.位置, Vec::new()));
                        }
                        外部块的项::外部结构体定义(s) => {
                            let 子项 = s
                                .字段列表
                                .iter()
                                .map(|f| 上下文.造(&f.名称, SymbolKind::FIELD, &f.位置, Vec::new()))
                                .collect();
                            结果.push(上下文.造(&s.名称, SymbolKind::STRUCT, &s.位置, 子项));
                        }
                        外部块的项::外部变量定义(v) => {
                            let 名 = 带前缀(&v.命名空间前缀, &v.名称);
                            结果.push(上下文.造(&名, SymbolKind::VARIABLE, &v.位置, Vec::new()));
                        }
                        外部块的项::外部常量(c) => {
                            结果.push(上下文.造(&c.名称, SymbolKind::CONSTANT, &c.位置, Vec::new()));
                        }
                        // 库引用与"别名→C符号"映射是元数据，不是可导航的符号
                        外部块的项::外部库声明(_)
                        | 外部块的项::外部函数映射声明(_)
                        | 外部块的项::外部变量映射声明(_) => {}
                    }
                }
            }
            顶层定义::导出块(块) => {
                for 项 in &块.项列表 {
                    match 项 {
                        导出块的项::导出函数(f) => {
                            结果.push(上下文.造(&f.名称, SymbolKind::FUNCTION, &f.位置, Vec::new()));
                        }
                        导出块的项::导出变量(v) => {
                            结果.push(上下文.造(&v.名称, SymbolKind::VARIABLE, &v.位置, Vec::new()));
                        }
                        // `导出: 名称` 无自身位置（引用此前定义），不产生新符号条目
                        导出块的项::导出引用(_) => {}
                    }
                }
            }
        }
    }

    // 启动程序（程序入口）—— 有独立位置，单列一项
    if let Some(入口) = &程序.入口 {
        结果.push(上下文.造("启动程序", SymbolKind::FUNCTION, &入口.位置, Vec::new()));
    }

    DocumentSymbolResponse::Nested(结果)
}

/// 造符号所需的三个只读上下文（词法只做一次，避免逐符号重复扫描）
struct 符号上下文<'a> {
    索引: &'a 行索引,
    配对: &'a HashMap<usize, usize>,
    词条列表: &'a [frontend::词法分析::词条],
}

fn 带前缀(前缀: &Option<String>, 名: &str) -> String {
    match 前缀 {
        Some(p) if !p.is_empty() => format!("{}.{}", p, 名),
        _ => 名.to_string(),
    }
}

impl<'a> 符号上下文<'a> {
    /// 造一个符号条目。
    /// `range` = 从**名字所在行**到**配对 `结束` 行**整段（无配对 ⇒ 仅该行）；`selection_range` = 名字本身。
    /// ⚠ 两者都必须走 `位置映射`（中文行的末列按 **UTF-16** 算），这是旧实现第二号 bug 所在。
    fn 造(
        &self,
        名称: &str,
        种类: SymbolKind,
        位置: &frontend::公共数据结构::源码位置,
        子项: Vec<DocumentSymbol>,
    ) -> DocumentSymbol {
        let 名字范围 = self
            .名字范围(名称, 位置)
            .or_else(|| self.索引.前端范围(位置.行号, 位置.列号, 名称.chars().count()));
        let 末行 = self.配对.get(&位置.行号).copied().unwrap_or(位置.行号);
        let 范围 = 定义行范围(self.索引, 位置.行号, 末行)
            .or(名字范围)
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        let 选中 = 名字范围.unwrap_or(范围);
        #[allow(deprecated)]
        DocumentSymbol {
            name: 名称.to_string(),
            detail: None,
            kind: 种类,
            tags: None,
            deprecated: None,
            range: 范围,
            selection_range: 选中,
            children: if 子项.is_empty() { None } else { Some(子项) },
        }
    }

    /// 导入模块条目：名字 = 代码里可用的名字，`detail` = 完整模块路径（见调用点注释）。
    fn 造模块(
        &self,
        名称: &str,
        完整路径: &str,
        位置: &frontend::公共数据结构::源码位置,
    ) -> DocumentSymbol {
        let mut 条目 = self.造(名称, SymbolKind::MODULE, 位置, Vec::new());
        条目.detail = Some(完整路径.to_string());
        条目
    }

    /// 在**声明所在行**用词条流取名字的精确列。
    ///
    /// ★ 为什么必须这样：AST 的 `位置` 是**声明起点** —— 对 `结构体/枚举/接口`（及外部结构体等）
    ///   而言它落在**关键字**那一列（实测各 `解析XXX定义` 都是"先取当前词条位置，再消耗名字"）。
    ///   直接把它当名字位置 ⇒ `selection_range` 会套在 `结构体` 上而不是 `学生` 上。
    ///   ⚠ 不能用"字符串 indexOf"代替：那会在名字恰为行内更早出现的子串时取错列。
    fn 名字范围(&self, 名称: &str, 位置: &frontend::公共数据结构::源码位置) -> Option<Range> {
        for 词 in self.词条列表.iter().filter(|t| t.位置.行号 == 位置.行号) {
            if 标识符文本(&词.类型).as_deref() == Some(名称) {
                return self
                    .索引
                    .前端范围(词.位置.行号, 词.位置.列号, 名称.chars().count());
            }
        }
        None
    }
}

/// 起行首列 → 末行尾列（都按 UTF-16 算）
fn 定义行范围(索引: &行索引, 起行1: usize, 末行1: usize) -> Option<Range> {
    let 起 = 索引.前端到LSP(起行1, 1)?;
    let 末字符数 = 索引.行字符(末行1.checked_sub(1)?)?.len();
    let 末 = 索引.前端到LSP(末行1, 末字符数 + 1)?;
    Some(Range::new(起, 末))
}

// ============================================================
// 查找所有引用
// ============================================================
/// 光标处的标识符在**本文件**内的全部引用。
///
/// `含声明` = LSP `context.includeDeclaration`；为 false 时剔除"声明位置"那一条。
/// 声明位置由 `定义跳转::查找定义` 反查（**复用**同一套解析，不另写一份作用域规则）。
pub fn 查找引用(
    源码: &str,
    文档URI: &Url,
    文件路径: Option<&Path>,
    位置: Position,
    含声明: bool,
) -> Vec<Location> {
    let 索引 = 行索引::新建(源码);
    let 词条列表 = 词法分析(源码);
    if 词条列表.is_empty() {
        return Vec::new();
    }

    // ── 光标处的标识符 + 它的"访问形态" ──
    let Some(光标下标) = 定位标识符(&索引, &词条列表, 位置) else {
        return Vec::new();
    };
    let 名 = 标识符文本(&词条列表[光标下标].类型);
    let Some(名) = 名 else { return Vec::new() };
    let 基础 = 成员基础(&词条列表, 光标下标); // Some(基础名) = 光标在 `基础.名` 的成员位置上

    // ── 收集同形态的全部出现 ──
    let mut 结果: Vec<Location> = Vec::new();
    for (i, 词) in 词条列表.iter().enumerate() {
        if 标识符文本(&词.类型).as_deref() != Some(名.as_str()) {
            continue; // 精确词条相等 ⇒ **不是子串匹配**（`计算结果` 不会因 `计算` 被命中）
        }
        let 本处基础 = 成员基础(&词条列表, i);
        let 同形态 = match (&基础, &本处基础) {
            (None, None) => true,                               // 裸名 ↔ 裸名
            (Some(基), Some(本处)) => 基 == 本处,                // 成员 ↔ **同一 base** 的成员
            _ => false,                                         // 裸名 vs 成员：不同符号，不混
        };
        if !同形态 {
            continue;
        }
        if let Some(范围) = 索引.前端范围(词.位置.行号, 词.位置.列号, 名.chars().count()) {
            结果.push(Location { uri: 文档URI.clone(), range: 范围 });
        }
    }

    // ── 跨文件：本文档之外、**能看到这个符号**的文件里的出现处 ──
    if let Some(文件) = 文件路径 {
        结果.extend(跨文件引用(源码, 文档URI, 文件, 位置, &名, 基础.as_deref()));
    }

    结果.sort_by_key(|l| (l.range.start.line, l.range.start.character));
    结果.dedup_by_key(|l| (l.range.start.line, l.range.start.character));

    // ── 按需剔除声明 ──
    if !含声明 {
        // ① **目标定义所在文件**里的那一份声明（可能在别的文件）—— 用**该文件的符号表**精确定位
        if let Some(文件) = 文件路径 {
            for 声明 in 声明位置(文件, &名) {
                结果.retain(|l| !(同位置(l, &声明)));
            }
        }
        // ② 本文档内由 `目标定义处` 反查到的声明（覆盖局部量等**符号表里没有**的情形）
        结果 = 剔除声明(源码, 文档URI, 文件路径, &结果, 位置);
    }
    结果
}

/// 两处 `Location` 是否指同一处代码。
///
/// ⚠ **不能直接比 `uri` 字符串**：`位置映射::文件URI` 产出的盘符是**小写**，
///   而编辑器发来的文档 URI 盘符大小写**依平台/版本而异**（本机实测 Node 给大写 `D:`）
///   ⇒ 同一个文件两边字符串不等，会让"剔除声明"静默失效（实测：跨文件声明剔不掉）。
///   盘符大小写在 Windows 上语义等价 ⇒ 比**路径**（各自 `to_file_path` 后再比）才可靠。
fn 同位置(a: &Location, b: &Location) -> bool {
    if a.range != b.range {
        return false;
    }
    match (a.uri.to_file_path(), b.uri.to_file_path()) {
        (Ok(x), Ok(y)) => x == y,
        // URI 不是文件（自定义 scheme）时才退回字符串比较
        _ => a.uri == b.uri,
    }
}

/// `Location` 是否属于 `文档` 这个文件（**忽略盘符大小写**，理由同 `同位置`）。
fn 同文档(l: &Location, 文档: &Url) -> bool {
    match (l.uri.to_file_path(), 文档.to_file_path()) {
        (Ok(x), Ok(y)) => x == y,
        _ => l.uri == *文档,
    }
}

/// 符号 `名` 在 `文件` 里的**声明位置**：读该文件、用 `文档符号` 取同名顶层条目的
/// `selection_range`（那正是**名字本身**的范围，不是关键字、也不是整块）。
///
/// ⚠ 为什么不用 `定义跳转` 反查：光标正落在**块头首词**上时，既定的"配对跳转"会
///   **优先**返回它配对的 `结束` 行 ⇒ 拿不到声明范围（实测：同一断言因此不通过）。
/// ⚠ 读的是**磁盘内容**（与其它文件的可达性判定同一口径）；未保存的编辑不参与。
fn 声明位置(文件: &Path, 名: &str) -> Vec<Location> {
    let Ok(文本) = std::fs::read_to_string(文件) else { return Vec::new() };
    let Some(uri) = crate::位置映射::文件URI(文件) else { return Vec::new() };
    match 文档符号(&文本) {
        DocumentSymbolResponse::Nested(列表) => 列表
            .iter()
            .filter(|s| s.name == 名)
            .map(|s| Location { uri: uri.clone(), range: s.selection_range })
            .collect(),
        _ => Vec::new(),
    }
}

/// 剔除"声明位置"那一条（LSP `includeDeclaration=false`）。
///
/// ⚠ 不能只用**光标处**反查声明：光标若正落在**块头首词**上（函数/结构体/枚举/接口的声明行），
///   第一阶段既定的"配对跳转"会**优先**返回它配对的 `结束` 行范围 ⇒ 拿不到声明范围。
///   ⇒ 依次用各个出现处反查，且**只接受"解析结果恰好是本次收集到的某个出现处"**的结果 ——
///   这条校验同时挡掉了上面那种"返回了别的语义"的情况。上限 5 次，避免长文件里白跑。
/// 声明反查复用 `定义跳转::查找定义`（**同一套作用域规则**，不另写）。
fn 剔除声明(
    源码: &str,
    文档URI: &Url,
    文件路径: Option<&Path>,
    出现处: &[Location],
    光标: Position,
) -> Vec<Location> {
    if 出现处.is_empty() {
        return Vec::new();
    }
    let mut 候选: Vec<Position> = Vec::with_capacity(6);
    候选.push(光标);
    // ⚠ 只用**本文档内**的出现处做候选：`目标定义处` 的入参是"本文档源码 + 位置"，
    //   拿别的文件的行列去解析本文档只会得到垃圾（跨文件那份声明由 `声明位置` 处理）
    候选.extend(
        出现处
            .iter()
            .filter(|l| 同文档(l, 文档URI))
            .take(5)
            .map(|l| l.range.start),
    );

    for 位置 in 候选 {
        // ⚠ 用 `目标定义处`（同文件 → **跨文件模块成员**），不是只用 `定义跳转`：
        //   本文档里的调用处是 `模块.名` 形态 ⇒ 只有跨文件那条链才解得开
        let 解析 = 目标定义处(源码, 文档URI, 文件路径, 位置);
        if 解析.is_empty() {
            continue;
        }
        let 是本次出现处 = 解析
            .iter()
            .any(|d| 出现处.iter().any(|l| l.range == d.range));
        if !是本次出现处 {
            continue; // 解析到了别的东西（如配对 `结束`）⇒ 不可信，换下一处再试
        }
        let 声明范围集: Vec<Range> = 解析.into_iter().map(|l| l.range).collect();
        return 出现处
            .iter()
            .filter(|l| !声明范围集.contains(&l.range))
            .cloned()
            .collect();
    }
    // 一处都反查不到声明（如 `自身.成员` 属第二阶段）⇒ 保持原样，不擅自删
    出现处.to_vec()
}

// ============================================================
// 跨文件引用（项目级扫描）
// ============================================================

/// 本文档之外、**能看到这个符号**的项目源文件里的出现处。
///
/// 与本文档内的匹配规则**不同且更严格**：本文档内是"同名词条 + 同访问形态"（纯词法），
/// 跨文件则额外要求 `跨文件跳转::文件可见性` 通过 —— 该文件的某个导入**确实能解析到这个符号**。
/// 没有这一步就是"**同名即算引用**"，会把毫不相干的模块里的同名函数全算进来。
///
/// ⚠ **不知道引用的是谁，就不猜**：目标定义解不出来 ⇒ 返回空。
/// ⚠ 只对**模块级符号**做：目标定义在别的文件（模块成员），或光标是裸名且本文档顶部
///   公开了同名符号。（`自身.成员`/实例成员的定义在本文档内 ⇒ 别的文件不可能导入它 ⇒ 不扫。）
/// ⚠ 成本：先逐文件**词法预筛**同名词条（很便宜），只有含该词条的文件才做导入解析；
///   所有文件共用**同一个**模块加载器（`已加载缓存` 跨文件共享）。
fn 跨文件引用(
    源码: &str,
    文档URI: &Url,
    文件路径: &Path,
    位置: Position,
    名: &str,
    基础: Option<&str>,
) -> Vec<Location> {
    let 目标 = 目标定义处(源码, 文档URI, Some(文件路径), 位置);
    let 本文档规范 = std::fs::canonicalize(文件路径).unwrap_or_else(|_| 文件路径.to_path_buf());
    let 定义在别处 = 目标
        .first()
        .and_then(|l| l.uri.to_file_path().ok())
        .map(|f| std::fs::canonicalize(&f).unwrap_or(f) != 本文档规范)
        .unwrap_or(false);

    let 该扫 = 定义在别处 || (基础.is_none() && 本文件有顶层符号(源码, 名));
    if !该扫 {
        return Vec::new();
    }
    let 目标文件 = if 定义在别处 {
        match 目标.first().and_then(|l| l.uri.to_file_path().ok()) {
            Some(f) => std::fs::canonicalize(&f).unwrap_or(f),
            None => return Vec::new(),
        }
    } else {
        本文档规范.clone()
    };

    let 文档目录 = 文件路径
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut 加载器 = crate::跨文件跳转::新建加载器(&文档目录);

    let mut 结果: Vec<Location> = Vec::new();
    for 候选 in crate::跨文件跳转::项目源文件(&文档目录) {
        if std::fs::canonicalize(&候选).unwrap_or_else(|_| 候选.clone()) == 本文档规范 {
            continue; // 本文档已由上面的"本文件词法扫描"覆盖
        }
        let Ok(文本) = std::fs::read_to_string(&候选) else { continue };
        let 词条列表 = 词法分析(&文本);
        // ① 词法预筛：不含同名词条的文件**连解析都不做**
        if !词条列表
            .iter()
            .any(|t| 标识符文本(&t.类型).as_deref() == Some(名))
        {
            continue;
        }
        // ② 该文件看不到这个符号 ⇒ 里面的同名词条不是对它的引用
        //   ⚠ 例外：**目标文件自己**对自己的顶层符号天然可见（裸名 + `本模块.名`）。
        //     不加这一条会漏掉**定义那一份** —— 定义文件通常**没有任何导入**，
        //     `文件可见性` 必然返回 None（实测：从调用处查引用只回了 3 处、少了定义）。
        let 候选规范 = std::fs::canonicalize(&候选).unwrap_or_else(|_| 候选.clone());
        let 可见 = if 候选规范 == 目标文件 {
            crate::跨文件跳转::可见性 {
                基础名: vec!["本模块".to_string()],
                可用裸名: true,
            }
        } else {
            let Some(可见) =
                crate::跨文件跳转::文件可见性(&文本, &候选, &mut 加载器, &目标文件, 名)
            else {
                continue;
            };
            可见
        };
        let Some(uri) = crate::位置映射::文件URI(&候选) else { continue };
        let 索引 = 行索引::新建(&文本);
        for (i, 词) in 词条列表.iter().enumerate() {
            if 标识符文本(&词.类型).as_deref() != Some(名) {
                continue;
            }
            // ③ 形态必须落在这个文件**可见的形态**内：`基础名.名`（基础名按该文件自己的
            //    导入算 ⇒ 原名与别名都对）或 `展开` 后的裸名
            let 算引用 = match 成员基础(&词条列表, i) {
                Some(本处基础) => 可见.基础名.iter().any(|b| *b == 本处基础),
                None => 可见.可用裸名,
            };
            if !算引用 {
                continue;
            }
            if let Some(范围) = 索引.前端范围(词.位置.行号, 词.位置.列号, 名.chars().count()) {
                结果.push(Location { uri: uri.clone(), range: 范围 });
            }
        }
    }
    结果
}

/// 光标符号的**目标定义位置**（沿既有链路解析，不另写一套规则）：
/// 同文件（`定义跳转`）→ 跨文件模块成员（`跨文件跳转`）。
/// ⚠ 不进 `成员解析`：`自身.成员`/实例成员定义在本文档内，对跨文件扫描无意义（见 `跨文件引用`）。
fn 目标定义处(
    源码: &str,
    文档URI: &Url,
    文件路径: Option<&Path>,
    位置: Position,
) -> Vec<Location> {
    let 同文件 = crate::定义跳转::查找定义(源码, 文档URI, 文件路径, 位置);
    if !同文件.is_empty() {
        return 同文件;
    }
    crate::跨文件跳转::查找定义(源码, 文档URI, 文件路径, 位置)
}

/// 本文档顶层是否有同名符号（函数/结构体/枚举/接口/变量/常量/导入命名空间）。
/// ⚠ 复用 `文档符号`（**已测过的**真语法树遍历），不另写一份 AST 匹配。
fn 本文件有顶层符号(源码: &str, 名: &str) -> bool {
    match 文档符号(源码) {
        DocumentSymbolResponse::Nested(列表) => 列表.iter().any(|s| s.name == 名),
        _ => false,
    }
}

/// 光标所在词条是否为"标识符类"，返回其在词条列表中的下标。
pub(crate) fn 定位标识符(索引: &行索引, 词条列表: &[frontend::词法分析::词条], 位置: Position) -> Option<usize> {
    let 行号 = 位置.line as usize + 1;
    let 列号 = 索引.LSP到前端(位置)?;
    // 光标可能停在词尾（VS Code 常见）⇒ 允许列号落在 `起..=起+长`
    for (i, 词) in 词条列表.iter().enumerate() {
        if 词.位置.行号 != 行号 {
            continue;
        }
        let Some(名) = 标识符文本(&词.类型) else { continue };
        let 起 = 词.位置.列号;
        let 止 = 起 + 名.chars().count(); // 右端**含**词尾后一格（容忍停在词尾）
        if 列号.1 >= 起 && 列号.1 <= 止 {
            return Some(i);
        }
    }
    None
}

/// 光标词条若处于 `基础.名` 的**成员**位置，返回基础名；否则 None（裸名）。
/// 基础名 = 紧邻 `点` 之前那个标识符类词条（如 `数学工具` / `本模块` / `自身` / `甲`）。
pub(crate) fn 成员基础(词条列表: &[frontend::词法分析::词条], 下标: usize) -> Option<String> {
    if 下标 < 2 {
        return None;
    }
    if !matches!(词条列表[下标 - 1].类型, 词条类型::点) {
        return None;
    }
    标识符文本(&词条列表[下标 - 2].类型)
}

/// 词条是否可作为"引用名"，并返回其文本。
/// 只认标识符类词条 ⇒ **注释（`文档注释内容`）与字符串（`文本字面量`）天然不是**，
/// 不可能出现"注释里同名字符串被当成引用"的假结果（旧实现的核心缺陷之一）。
pub(crate) fn 标识符文本(t: &词条类型) -> Option<String> {
    match t {
        词条类型::标识符(s) | 词条类型::类型关键字(s) | 词条类型::外部标识符(s) => Some(s.clone()),
        词条类型::本模块 => Some("本模块".to_string()),
        词条类型::自身 => Some("自身".to_string()),
        _ => None,
    }
}

// ============================================================
// 共享前端调用
// ============================================================
/// 词法分析（**带内容缓存**）。本函数固定用路径 `<编辑器>`；读**别的文件**时请用
/// `crate::缓存::词法分析(真实路径, 文本)` —— 路径参与缓存键（词条里内嵌该路径）。
pub(crate) fn 词法分析(源码: &str) -> Rc<Vec<frontend::词法分析::词条>> {
    crate::缓存::词法分析("<编辑器>", 源码)
}

/// ⚠ **允许带诊断**：编辑器里代码常是半成品；只要解析出了顶层定义列表就可用
///   （比"一处报错 ⇒ 大纲全空"更贴合真实使用）。
/// **带内容缓存**（同一份源码在多个请求里只解析一次）。
pub(crate) fn 解析(源码: &str) -> Rc<程序> {
    crate::缓存::解析("<编辑器>", 源码)
}

/// 解析**别的文件**：路径必须传真实路径 —— 它参与缓存键，也决定 AST 里 `源码位置.文件路径`
/// （跨文件跳转靠它判断"定义在哪个文件"）。
pub(crate) fn 解析带路径(文件路径: &str, 源码: &str) -> Rc<程序> {
    crate::缓存::解析(文件路径, 源码)
}
