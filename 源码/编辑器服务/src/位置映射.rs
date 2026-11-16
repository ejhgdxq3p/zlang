// 文件: 编辑器服务/src/位置映射.rs
// 职责: **源码文本 ↔ LSP 位置 的唯一转换点**（跳转方案 §10.1 的"第一号技术债"）
//
// ⚠ 三套"列"互不相等，混用必然跳错位置（改造前的实现正是把 ① 当 ③ 用）：
//   ① 前端 `源码位置.列号`   = **字符列，1-based**（`词法分析器` 每 `消耗()` 一个 char 就 +1）
//   ② Rust 字节偏移          = UTF-8 字节
//   ③ LSP `Position.character` = **UTF-16 code unit，0-based**
// 中文（BMP）UTF-8 占 3 字节 / UTF-16 占 1 unit；emoji（增补平面）UTF-8 占 4 字节 / UTF-16 占 2 units
// ⇒ 只对纯 ASCII 三者才巧合相等。本仓库**路径与标识符几乎全中文**，故必须显式转换。

use lsp_types::{Position, Range, Url};

/// 单行数据（字符序列 + 该行在源码中的起始字节偏移）
struct 行数据 {
    字符: Vec<char>,
    行起始字节: usize,
}

/// 行索引：按行预切分源码，提供三套坐标的互转。
///
/// 等价于 rust-analyzer 的 `LineIndex` / clangd 的 `SourceManager`：**必须先建索引再查询**，
/// 否则每次查询都要重扫全文。
pub struct 行索引 {
    行: Vec<行数据>,
}

impl 行索引 {
    /// 按 `\n` 切行（`\r\n` 的 `\r` 归入行尾并剔除，不计入列）
    pub fn 新建(源码: &str) -> Self {
        let mut 行 = Vec::new();
        let 字节 = 源码.as_bytes();
        let mut 当前起始 = 0usize;
        let mut i = 0usize;
        loop {
            let 到头 = i >= 字节.len();
            if 到头 || 字节[i] == b'\n' {
                // 只在字符边界切片：`当前起始` 是 0 或 `\n` 之后；`i` 是 `\n` 处或末尾
                let mut 行文本 = &源码[当前起始..i];
                if 行文本.ends_with('\r') {
                    行文本 = &行文本[..行文本.len() - 1];
                }
                行.push(行数据 { 字符: 行文本.chars().collect(), 行起始字节: 当前起始 });
                if 到头 {
                    break;
                }
                当前起始 = i + 1;
            }
            i += 1;
        }
        行索引 { 行 }
    }

    pub fn 行数(&self) -> usize {
        self.行.len()
    }

    /// ① 前端 `源码位置`（1-based 行、1-based **字符列**）⇒ ③ LSP Position（0-based 行、**UTF-16 列**）
    pub fn 前端到LSP(&self, 行号1: usize, 字符列1: usize) -> Option<Position> {
        let 行 = self.行.get(行号1.checked_sub(1)?)?;
        let 字符下标 = 字符列1.checked_sub(1)?;
        if 字符下标 > 行.字符.len() {
            return None;
        }
        Some(Position::new((行号1 - 1) as u32, 取utf16列(&行.字符, 字符下标) as u32))
    }

    /// ③ LSP Position ⇒ ① 前端（1-based 行号, 1-based 字符列）
    pub fn LSP到前端(&self, 位置: Position) -> Option<(usize, usize)> {
        let 字符下标 = self.行列到字符下标(位置.line as usize, 位置.character as usize)?;
        Some((位置.line as usize + 1, 字符下标 + 1))
    }

    /// ③ LSP Position ⇒ ② UTF-8 字节偏移
    pub fn LSP到字节偏移(&self, 位置: Position) -> Option<usize> {
        let 行 = self.行.get(位置.line as usize)?;
        let 字符下标 = 由utf16列取字符下标(&行.字符, 位置.character as usize)?;
        let 行内字节: usize = 行.字符[..字符下标].iter().map(|c| c.len_utf8()).sum();
        Some(行.行起始字节 + 行内字节)
    }

    /// ① 前端位置 + **字符数** ⇒ ③ LSP Range（用于把"一个词条"包成可高亮的区间）
    pub fn 前端范围(&self, 行号1: usize, 字符列1: usize, 字符数: usize) -> Option<Range> {
        let 起 = self.前端到LSP(行号1, 字符列1)?;
        let 行 = self.行.get(行号1.checked_sub(1)?)?;
        let 起下标 = 字符列1.checked_sub(1)?;
        let 末下标 = 起下标.saturating_add(字符数).min(行.字符.len());
        Some(Range::new(起, Position::new(起.line, 取utf16列(&行.字符, 末下标) as u32)))
    }

    /// ③ LSP Position ⇒ 该行内的**字符下标**（供"取光标处标识符"使用；落在代理对中间时向下取整）
    pub fn 行列到字符下标(&self, 行0: usize, utf16列: usize) -> Option<usize> {
        let 行 = self.行.get(行0)?;
        由utf16列取字符下标(&行.字符, utf16列)
    }

    /// 该行的字符序列（0-based 行号）
    pub fn 行字符(&self, 行0: usize) -> Option<&[char]> {
        self.行.get(行0).map(|d| d.字符.as_slice())
    }

    /// 该行在源码中的起始字节偏移（0-based 行号）
    pub fn 行起始字节(&self, 行0: usize) -> Option<usize> {
        self.行.get(行0).map(|d| d.行起始字节)
    }
}

/// 字符下标 ⇒ UTF-16 列
fn 取utf16列(字符: &[char], 字符下标: usize) -> usize {
    字符[..字符下标.min(字符.len())].iter().map(|c| c.len_utf16()).sum()
}

/// UTF-16 列 ⇒ 字符下标。
/// **绝不 panic**：列越界 ⇒ 返回行尾；列落在代理对中间 ⇒ 向下取整到该字符起点。
fn 由utf16列取字符下标(字符: &[char], utf16列: usize) -> Option<usize> {
    let mut 累计 = 0usize;
    for (下标, c) in 字符.iter().enumerate() {
        if 累计 >= utf16列 {
            return Some(下标);
        }
        累计 += c.len_utf16();
    }
    Some(字符.len())
}

/// 文件路径 → `file://` URI，并**把盘符规范成小写**（`file:///D:/…` → `file:///d:/…`）。
///
/// ★ 为什么必须有这一步：编辑器侧（VS Code / CodeBuddy）发出的文档 URI 盘符是**小写**
///   （`vscode.Uri.file()` 的结果），而 `Url::from_file_path` 会**原样保留** `Path` 里的大小写
///   （Windows 上 `d:\…` 经 `canonicalize` 后常变成 `D:\…`）。两者大小写不一致会让
///   **URI 等值判断失效** —— 编辑器据此判断"跳转目标是不是当前文档"，不一致时可能把结果
///   当成另一个文件（实测：本机用例里跨文件跳转的 URI 与编辑器 `pathToFileURL` 的结果
///   仅差盘符大小写，等值判断直接为 false）。
///   盘符大小写在 Windows 上语义等价，统一成**小写**是安全的（与编辑器同形）。
///
/// ⚠ 只动 `file:///<盘符>:` 的第 4 个字符，路径其余部分（含中文的百分号编码）保持不变。
pub fn 文件URI(路径: &std::path::Path) -> Option<Url> {
    let uri = Url::from_file_path(路径).ok()?;
    match 小写盘符(&uri.to_string()) {
        // 重解析失败也退回原 URI（宁可大小写不一致，也不要丢结果）
        Some(文本) => Some(Url::parse(&文本).unwrap_or(uri)),
        None => Some(uri),
    }
}

/// 把 `file:///X:/…` 的盘符改成小写；无需改动时返回 `None`（调用方沿用原 URI）。
fn 小写盘符(文本: &str) -> Option<String> {
    let 起 = 文本.find(":///")? + 4;
    let 字节 = 文本.as_bytes();
    if 字节.len() <= 起 + 1 || !字节[起].is_ascii_uppercase() || 字节[起 + 1] != b':' {
        return None;
    }
    let mut 新 = String::with_capacity(文本.len());
    新.push_str(&文本[..起]);
    新.push(字节[起].to_ascii_lowercase() as char);
    新.push_str(&文本[起 + 1..]);
    Some(新)
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 中文列必须按_utf16_而非字符数或字节数() {
        //          字符列: 1=甲 2=空 3='=' 4=空 5=乙
        let 索引 = 行索引::新建("甲 = 乙\n");
        // 前端 1-based 字符列 5（`乙`）⇒ LSP utf16 列 4
        let 位置 = 索引.前端到LSP(1, 5).expect("应能转换");
        assert_eq!(位置.line, 0);
        assert_eq!(位置.character, 4, "UTF-16 列应为 4（中文各占 1 unit），而非 4 字节偏移或别的值");
        // 反向：LSP ⇒ 前端
        assert_eq!(索引.LSP到前端(Position::new(0, 4)), Some((1, 5)));
        // 字节偏移：`乙` 前有 "甲 = " 共 3+1+1+1 = 6 字节
        assert_eq!(索引.LSP到字节偏移(Position::new(0, 4)), Some(6));
    }

    #[test]
    fn 增补平面字符占两个_utf16_unit() {
        // "😀甲"：😀 = 1 char / 4 bytes / 2 utf16 units；甲 = 1 char
        let 索引 = 行索引::新建("😀甲");
        assert_eq!(索引.前端到LSP(1, 1), Some(Position::new(0, 0)));
        assert_eq!(索引.前端到LSP(1, 2), Some(Position::new(0, 2)), "😀 之后是第 2 个 utf16 列");
        assert_eq!(索引.LSP到前端(Position::new(0, 2)), Some((1, 2)));
        // 字节偏移：😀 占 4 字节
        assert_eq!(索引.LSP到字节偏移(Position::new(0, 2)), Some(4));
    }

    #[test]
    fn 越界与代理对中间均不_panic() {
        let 索引 = 行索引::新建("甲乙\n丙\n");
        assert_eq!(索引.行数(), 3, "末尾换行会产生一个空行");
        // 列越过行尾 ⇒ 落在行尾，不 panic
        assert_eq!(索引.行字符(0).map(|c| c.len()), Some(2));
        assert_eq!(索引.行列到字符下标(0, 999), Some(2));
        // 行号越界 ⇒ None（不 panic）
        assert_eq!(索引.前端到LSP(99, 1), None);
        assert_eq!(索引.LSP到前端(Position::new(99, 0)), None);
        // 字符列 0（非法 1-based）⇒ None
        assert_eq!(索引.前端到LSP(1, 0), None);
    }

    #[test]
    fn 范围含中文时末列按_utf16_计算() {
        let 索引 = 行索引::新建("甲 = 调用(甲)\n");
        // 从字符列 5 起取 2 个字符（"调用"）⇒ utf16 [4,6)
        let 范围 = 索引.前端范围(1, 5, 2).expect("应能转换");
        assert_eq!(范围.start, Position::new(0, 4));
        assert_eq!(范围.end, Position::new(0, 6));
    }

    #[test]
    fn 往返一致_前百列全部可逆() {
        let 源码 = "结构体 学生(文本 姓名, 整数 年龄):\n    自身.姓名 = 姓名\n结束\n😀 = 1\n";
        let 索引 = 行索引::新建(源码);
        for 行0 in 0..索引.行数() {
            let 字符数 = 索引.行字符(行0).map(|c| c.len()).unwrap_or(0);
            for 字符列1 in 1..=字符数 {
                let lsp = 索引.前端到LSP(行0 + 1, 字符列1).expect("前向转换");
                let 回 = 索引.LSP到前端(lsp).expect("反向转换");
                assert_eq!(回, (行0 + 1, 字符列1), "行{} 字符列{} 往返不一致（LSP={:?}）", 行0 + 1, 字符列1, lsp);
            }
        }
    }
}
