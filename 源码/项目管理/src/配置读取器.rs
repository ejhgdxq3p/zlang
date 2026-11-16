// 文件: 项目管理/src/配置读取器.rs
// 职责: 统一配置解析器 — 解析/序列化 `.配置` 格式文件
// 规范 §6.1.3：智语言配置文件格式
//   注释: // 单行 / /* */ 多行（不嵌套）
//   键: 汉字 + 数字 + 下划线，裸写不加引号
//   值: 字符串(""), 整数, 小数, 逻辑(真/假), 数组([])
//   字符串转义: \" \\ \n \t
//   表头: [段名] 裸写不加引号
// 双模式共享：项目管理 crate 唯一配置真相源，版本管理器消费

use std::path::Path;

// ═══════════════════════════════════════════════════
// 数据结构
// ═══════════════════════════════════════════════════

/// 解析后的配置文档
#[derive(Debug, Clone)]
pub struct 配置文档 {
    /// 第一个表头前的键值对（顶层段）
    pub 顶层: Vec<(String, 配置值)>,
    /// 按出现顺序排列的配置段
    pub 段: Vec<配置段>,
}

#[derive(Debug, Clone)]
pub struct 配置段 {
    pub 段名: String,
    pub 键值: Vec<(String, 配置值)>,
}

/// 配置值类型，直接对应规范 §6.1.3 + 6.2.2 依赖对象形式
/// 表（内联对象）供 [运行依赖] 等段写 `代号 = { 类型 = "外部语言", 原始名 = "sqlite3", ... }`
#[derive(Debug, Clone, PartialEq)]
pub enum 配置值 {
    文本(String),
    整数(i64),
    小数(f64),
    逻辑(bool),
    数组(Vec<配置值>),
    表(Vec<(String, 配置值)>),
}

// ═══════════════════════════════════════════════════
// 解析器 — 解析配置文本
// ═══════════════════════════════════════════════════

/// 解析 .配置 文本为通用中间表示
pub fn 解析配置(文本: &str) -> 配置文档 {
    // 容错 UTF-8 BOM：U+FEFF 不是空白字符，行级 trim 剥不掉；
    // 带 BOM 文件首行 `\u{feff}[段名]` 会因不以 `[` 开头被"容错忽略"整段吞掉
    // （Windows 记事本 / PowerShell Set-Content 默认带 BOM，必须容错）
    let 文本 = 文本.trim_start_matches('\u{feff}');
    // 第一步：预处理 — 移除多行注释 /* */
    let 清洗文本 = 移除多行注释(文本);

    let mut 文档 = 配置文档 { 顶层: vec![], 段: vec![] };
    let mut 当前段键值: Option<&mut Vec<(String, 配置值)>> = None;

    for 行 in 清洗文本.lines() {
        let 行原 = 行;
        let 行 = 行.trim();

        // 跳过空行
        if 行.is_empty() {
            continue;
        }

        // 单行注释（// 或 # 向后兼容）
        if 行.starts_with("//") || 行.starts_with('#') {
            continue;
        }

        // 表头 [段名]（兼容旧 "段名"）
        if let Some(内) = 行.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let 段名 = 去引号(内);
            文档.段.push(配置段 { 段名, 键值: vec![] });
            当前段键值 = Some(&mut 文档.段.last_mut().unwrap().键值);
            continue;
        }

        // 键值对
        if let Some((k, v)) = 行.split_once('=') {
            let 键 = 去引号(k.trim());
            let 值 = 解析值(v.trim());
            match &mut 当前段键值 {
                Some(段) => 段.push((键, 值)),
                None => 文档.顶层.push((键, 值)),
            }
            continue;
        }

        // 其他行：容错忽略
        let _ = 行原;
    }

    文档
}

/// 移除多行注释 /* */（不支持嵌套）。保留换行符以保持行号对齐
fn 移除多行注释(文本: &str) -> String {
    let mut 结果 = String::with_capacity(文本.len());
    let mut 字符 = 文本.chars().peekable();
    while let Some(c) = 字符.next() {
        if c == '/' && 字符.peek() == Some(&'*') {
            字符.next(); // 吞掉 '*'
            // 跳过直到 */
            loop {
                match 字符.next() {
                    Some('*') if 字符.peek() == Some(&'/') => {
                        字符.next(); // 吞掉 '/'
                        break;
                    }
                    Some('\n') => 结果.push('\n'), // 保持行号
                    Some(_) => continue,
                    None => break, // 未闭合：容错
                }
            }
        } else {
            结果.push(c);
        }
    }
    结果
}

/// 解析单个值文本为 配置值
fn 解析值(s: &str) -> 配置值 {
    // 0. 表（内联对象）：{ 键 = 值, ... } — 规范 6.2.2 依赖对象形式
    if s.starts_with('{') && s.ends_with('}') {
        let 内 = &s[1..s.len() - 1];
        if 内.trim().is_empty() {
            return 配置值::表(vec![]);
        }
        let 条目: Vec<(String, 配置值)> = 拆分数组元素(内)
            .into_iter()
            .filter_map(|对| {
                let (k, v) = 对.split_once('=')?;
                Some((去引号(k.trim()), 解析值(v.trim())))
            })
            .collect();
        return 配置值::表(条目);
    }

    // 1. 数组：[...]
    if s.starts_with('[') && s.ends_with(']') {
        let 内 = &s[1..s.len() - 1];
        if 内.trim().is_empty() {
            return 配置值::数组(vec![]);
        }
        let 元素: Vec<配置值> = 拆分数组元素(内)
            .into_iter()
            .map(|e| 解析值(e.trim()))
            .collect();
        return 配置值::数组(元素);
    }

    // 2. 字符串："..."
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let 内 = &s[1..s.len() - 1];
        return 配置值::文本(解析转义(内));
    }

    // 3. 逻辑（优先于整数，避免 "真" 被 parse 失败再回退）
    if s == "真" || s == "true" {
        return 配置值::逻辑(true);
    }
    if s == "假" || s == "false" {
        return 配置值::逻辑(false);
    }

    // 4. 小数（含小数点）
    if s.contains('.') {
        if let Ok(v) = s.parse::<f64>() {
            return 配置值::小数(v);
        }
    }

    // 5. 整数
    if let Ok(v) = s.parse::<i64>() {
        return 配置值::整数(v);
    }

    // 6. 回退为文本（兼容旧格式裸值）
    配置值::文本(s.to_string())
}

/// 解析字符串中的转义序列
fn 解析转义(s: &str) -> String {
    let mut 结果 = String::with_capacity(s.len());
    let mut 字符 = s.chars();
    while let Some(c) = 字符.next() {
        if c == '\\' {
            match 字符.next() {
                Some('"') => 结果.push('"'),
                Some('\\') => 结果.push('\\'),
                Some('n') => 结果.push('\n'),
                Some('t') => 结果.push('\t'),
                Some(x) => {
                    结果.push('\\');
                    结果.push(x);
                }
                None => 结果.push('\\'),
            }
        } else {
            结果.push(c);
        }
    }
    结果
}

/// 拆分数组元素（简单逗号分隔，不支持嵌套逗号内逗号）
fn 拆分数组元素(内: &str) -> Vec<String> {
    let mut 元素 = Vec::new();
    let mut 当前 = String::new();
    let mut 引号中 = false;
    for c in 内.chars() {
        match c {
            '"' => {
                引号中 = !引号中;
                当前.push(c);
            }
            ',' if !引号中 => {
                元素.push(当前.trim().to_string());
                当前.clear();
            }
            _ => 当前.push(c),
        }
    }
    let 尾 = 当前.trim().to_string();
    if !尾.is_empty() {
        元素.push(尾);
    }
    元素
}

// ═══════════════════════════════════════════════════
// 序列化器
// ═══════════════════════════════════════════════════

/// 将配置文档序列化为规范 .配置 格式
pub fn 序列化配置文档(文档: &配置文档) -> String {
    let mut 结果 = String::new();

    // 顶层键值对
    for (键, 值) in &文档.顶层 {
        写键值(&mut 结果, 键, 值);
    }
    if !文档.顶层.is_empty() {
        结果.push('\n');
    }

    // 各段
    for 段 in &文档.段 {
        结果.push_str(&format!("[{}]\n", 段.段名));
        for (键, 值) in &段.键值 {
            写键值(&mut 结果, 键, 值);
        }
        结果.push('\n');
    }

    结果
}

fn 写键值(s: &mut String, 键: &str, 值: &配置值) {
    s.push_str(键);
    s.push_str(" = ");
    写值(s, 值);
    s.push('\n');
}

/// 将单个值序列化为文本（pub(crate) 供同 crate 其他模块调用，如 产物路径.rs）
pub(crate) fn 写值(s: &mut String, 值: &配置值) {
    match 值 {
        配置值::文本(t) => {
            s.push('"');
            for c in t.chars() {
                match c {
                    '"' => s.push_str("\\\""),
                    '\\' => s.push_str("\\\\"),
                    '\n' => s.push_str("\\n"),
                    '\t' => s.push_str("\\t"),
                    x => s.push(x),
                }
            }
            s.push('"');
        }
        配置值::整数(n) => {
            s.push_str(&n.to_string());
        }
        配置值::小数(f) => {
            s.push_str(&f.to_string());
        }
        配置值::逻辑(true) => s.push_str("真"),
        配置值::逻辑(false) => s.push_str("假"),
        配置值::数组(a) => {
            s.push('[');
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                写值(s, v);
            }
            s.push(']');
        }
        配置值::表(条目) => {
            s.push('{');
            for (i, (k, v)) in 条目.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(k);
                s.push_str(" = ");
                写值(s, v);
            }
            s.push('}');
        }
    }
}

// ═══════════════════════════════════════════════════
// 便利取值函数
// ═══════════════════════════════════════════════════

/// 取指定段名的段（第一个匹配）
pub fn 取段<'a>(文档: &'a 配置文档, 段名: &str) -> Option<&'a 配置段> {
    文档.段.iter().find(|段| 段.段名 == 段名)
}

/// 取段的第一个匹配键的文本值
pub fn 取文本(段: &配置段, 键: &str) -> Option<String> {
    段.键值
        .iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::文本(s) => Some(s.clone()),
            _ => None,
        })
}

/// 取段的第一个匹配键的整数值
pub fn 取整数(段: &配置段, 键: &str) -> Option<i64> {
    段.键值
        .iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::整数(n) => Some(*n),
            _ => None,
        })
}

/// 取段的第一个匹配键的小数值
pub fn 取小数(段: &配置段, 键: &str) -> Option<f64> {
    段.键值
        .iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::小数(f) => Some(*f),
            _ => None,
        })
}

/// 取段的第一个匹配键的逻辑值
pub fn 取逻辑(段: &配置段, 键: &str) -> Option<bool> {
    段.键值
        .iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::逻辑(b) => Some(*b),
            _ => None,
        })
}

/// 取段的第一个匹配键的数组值
pub fn 取数组<'a>(段: &'a 配置段, 键: &str) -> Option<&'a Vec<配置值>> {
    段.键值
        .iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::数组(a) => Some(a),
            _ => None,
        })
}

/// 取段的第一个匹配键的表值（内联对象）— 规范 6.2.2 依赖对象形式
pub fn 取表<'a>(段: &'a 配置段, 键: &str) -> Option<&'a Vec<(String, 配置值)>> {
    段.键值
        .iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::表(t) => Some(t),
            _ => None,
        })
}

/// 从表值中取指定键的文本
pub fn 表取文本(表: &[(String, 配置值)], 键: &str) -> Option<String> {
    表.iter()
        .find(|(k, _)| k == 键)
        .and_then(|(_, v)| match v {
            配置值::文本(s) => Some(s.clone()),
            _ => None,
        })
}

/// 从文件路径读取并解析 .配置 文件
pub fn 解析文件(路径: &Path) -> Result<配置文档, String> {
    let 内容 =
        std::fs::read_to_string(路径).map_err(|e| format!("读取配置失败: {}", e))?;
    Ok(解析配置(&内容))
}

/// 写入配置文档到文件
pub fn 写入文件(路径: &Path, 文档: &配置文档) -> std::io::Result<()> {
    if let Some(父) = 路径.parent() {
        std::fs::create_dir_all(父)?;
    }
    std::fs::write(路径, 序列化配置文档(文档))
}

// ═══════════════════════════════════════════════════
// 内部工具
// ═══════════════════════════════════════════════════

/// 去除首尾引号（兼容旧格式 `"键"` / `"值"`）
fn 去引号(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"'))
            || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

// ═══════════════════════════════════════════════════
// 锁文件专用（项目依赖.配置）— 以段为条目的 HashMap 序列化
// ═══════════════════════════════════════════════════

/// 将锁数据（HashMap<库名, Vec<(键名, 值文本)>>）解析为配置文档
pub fn 从段映射到文档(映射: &std::collections::HashMap<String, Vec<(String, String)>>) -> 配置文档 {
    let mut 文档 = 配置文档 {
        顶层: vec![],
        段: vec![],
    };
    let mut 键名顺序: Vec<&String> = 映射.keys().collect();
    键名顺序.sort();
    for 库名 in 键名顺序 {
        if let Some(条目映射) = 映射.get(库名) {
            let mut 键值 = Vec::new();
            for (k, v) in 条目映射 {
                键值.push((k.clone(), 配置值::文本(v.clone())));
            }
            文档.段.push(配置段 {
                段名: 库名.clone(),
                键值,
            });
        }
    }
    文档
}

/// 将配置文档转换回段映射（每段的全部键值对）
pub fn 文档到段映射(文档: &配置文档) -> std::collections::HashMap<String, Vec<(String, String)>> {
    let mut 映射 = std::collections::HashMap::new();
    for 段 in &文档.段 {
        let 条目: Vec<(String, String)> = 段
            .键值
            .iter()
            .filter_map(|(k, v)| match v {
                配置值::文本(t) => Some((k.clone(), t.clone())),
                _ => {
                    // 对于非文本值，用序列化后去引号方式
                    let mut buf = String::new();
                    写值(&mut buf, v);
                    Some((k.clone(), 去引号(&buf)))
                }
            })
            .collect();
        映射.insert(段.段名.clone(), 条目);
    }
    文档.顶层.iter().for_each(|(k, v)| {
        let mut buf = String::new();
        写值(&mut buf, v);
        映射
            .entry(String::new())
            .or_default()
            .push((k.clone(), 去引号(&buf)));
    });
    映射
}

// ═══════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 单行注释_双斜杠() {
        let 文档 = 解析配置("键 = \"值\"\n// 这是注释\n其他 = \"测试\"");
        assert_eq!(文档.顶层.len(), 2);
        assert_eq!(文档.顶层[0].0, "键");
        assert_eq!(文档.顶层[0].1, 配置值::文本("值".into()));
        assert_eq!(文档.顶层[1].0, "其他");
        assert_eq!(文档.顶层[1].1, 配置值::文本("测试".into()));
    }

    #[test]
    fn 单行注释_井号兼容() {
        let 文档 = 解析配置("# 旧注释\n键 = \"值\"");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.顶层[0].1, 配置值::文本("值".into()));
    }

    #[test]
    fn 多行注释_单行内() {
        let 文档 = 解析配置("键 = /* 注释 */ \"值\"");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.顶层[0].1, 配置值::文本("值".into()));
    }

    #[test]
    fn 多行注释_跨行() {
        let 文档 = 解析配置("/* 行1\n行2 */ 键 = \"值\"");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.顶层[0].1, 配置值::文本("值".into()));
    }

    #[test]
    fn 多行注释_未闭合() {
        let 文档 = 解析配置("键 = \"值\"\n/* 不闭合");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.顶层[0].1, 配置值::文本("值".into()));
    }

    #[test]
    fn 字符串转义() {
        let 文档 = 解析配置("s = \"a\\nb\\tc\\\"d\"");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.顶层[0].1, 配置值::文本("a\nb\tc\"d".into()));
    }

    #[test]
    fn 逻辑_中文() {
        let 文档 = 解析配置("a = 真\nb = 假");
        assert_eq!(文档.顶层.len(), 2);
        assert_eq!(文档.顶层[0].1, 配置值::逻辑(true));
        assert_eq!(文档.顶层[1].1, 配置值::逻辑(false));
    }

    #[test]
    fn 逻辑_英文兼容() {
        let 文档 = 解析配置("a = true\nb = false");
        assert_eq!(文档.顶层[0].1, 配置值::逻辑(true));
        assert_eq!(文档.顶层[1].1, 配置值::逻辑(false));
    }

    #[test]
    fn 整数() {
        let 文档 = 解析配置("n = 42");
        assert_eq!(文档.顶层[0].1, 配置值::整数(42));
    }

    #[test]
    fn 小数() {
        let 文档 = 解析配置("f = 3.14");
        assert_eq!(文档.顶层[0].1, 配置值::小数(3.14));
    }

    #[test]
    fn 数组() {
        let 文档 = 解析配置("a = [\"x\", \"y\"]");
        if let 配置值::数组(ref 元素) = 文档.顶层[0].1 {
            assert_eq!(元素.len(), 2);
            assert_eq!(元素[0], 配置值::文本("x".into()));
            assert_eq!(元素[1], 配置值::文本("y".into()));
        } else {
            panic!("期望数组");
        }
    }

    #[test]
    fn 空数组() {
        let 文档 = 解析配置("a = []");
        if let 配置值::数组(ref 元素) = 文档.顶层[0].1 {
            assert!(元素.is_empty());
        } else {
            panic!("期望数组");
        }
    }

    #[test]
    fn 顶层键值对() {
        let 文档 = 解析配置("顶级 = \"v\"\n[段]\nk = 1");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.段.len(), 1);
        assert_eq!(文档.段[0].段名, "段");
    }

    #[test]
    fn 段顺序保持() {
        let 文档 = 解析配置("[乙]\nx = 1\n[甲]\ny = 2");
        assert_eq!(文档.段[0].段名, "乙");
        assert_eq!(文档.段[1].段名, "甲");
    }

    #[test]
    fn 空文件() {
        let 文档 = 解析配置("");
        assert!(文档.顶层.is_empty());
        assert!(文档.段.is_empty());
    }

    #[test]
    fn 键值空格容差() {
        let 文档 = 解析配置("名称=\"值\"\n[段]\n键 =  \"测试\" ");
        assert_eq!(文档.顶层[0].0, "名称");
        assert_eq!(文档.顶层[0].1, 配置值::文本("值".into()));
        assert_eq!(文档.段[0].键值[0].0, "键");
        assert_eq!(文档.段[0].键值[0].1, 配置值::文本("测试".into()));
    }

    #[test]
    fn 序列化_往返() {
        let 原 = "// 配置\n[项目]\n名称 = \"测试\"\n启用 = 真\n计数 = 42\n";
        let 文档 = 解析配置(原);
        let 输出 = 序列化配置文档(&文档);
        let 再解析 = 解析配置(&输出);
        // 验证段数一致
        assert_eq!(再解析.段.len(), 文档.段.len());
        // 验证段名一致
        assert_eq!(再解析.段[0].段名, "项目");
        // 验证各值类型
        assert_eq!(再解析.段[0].键值.len(), 3);
        // 验证序列化产出 .配置 格式（不应有引号键）
        assert!(!输出.contains("\"项目\""), "段名不应带引号: {}", 输出);
        assert!(!输出.contains("\"名称\""), "键不应带引号: {}", 输出);
        assert!(输出.contains("真") || 输出.contains("假"), "逻辑应为中文: {}", 输出);
    }

    #[test]
    fn 段名兼容引号() {
        let 文档 = 解析配置("[\"旧段\"]\n键 = \"值\"");
        assert_eq!(文档.段[0].段名, "旧段");
        assert_eq!(文档.段[0].键值[0].0, "键");
    }

    #[test]
    fn 文本中逗号不干扰数组() {
        let 文档 = 解析配置("列表 = [\"a, b\", \"c\"]");
        if let 配置值::数组(ref 元素) = 文档.顶层[0].1 {
            assert_eq!(元素.len(), 2);
        } else {
            panic!("期望数组");
        }
    }

    #[test]
    fn 内联对象_解析() {
        let 文档 = 解析配置("[运行依赖]\n数据库 = { 原始名 = \"sqlite3\", 版本 = \"3.42.0\", 管理方式 = \"手动\", 类型 = \"外部语言\" }");
        let 段 = 取段(&文档, "运行依赖").unwrap();
        let 表 = 取表(段, "数据库").unwrap();
        assert_eq!(表取文本(表, "原始名").as_deref(), Some("sqlite3"));
        assert_eq!(表取文本(表, "版本").as_deref(), Some("3.42.0"));
        assert_eq!(表取文本(表, "管理方式").as_deref(), Some("手动"));
        assert_eq!(表取文本(表, "类型").as_deref(), Some("外部语言"));
    }

    #[test]
    fn 内联对象_嵌套数组() {
        // 数组内元素含引号内逗号不应截断
        let 文档 = 解析配置("[段]\n数学 = { 版本 = \">= 1.0.0, < 2.0.0\" }");
        let 段 = 取段(&文档, "段").unwrap();
        let 表 = 取表(段, "数学").unwrap();
        assert_eq!(表取文本(表, "版本").as_deref(), Some(">= 1.0.0, < 2.0.0"));
    }

    #[test]
    fn 内联对象_往返() {
        let 原 = "[运行依赖]\n数据库 = { 类型 = \"外部语言\", 原始名 = \"sqlite3\", 版本 = \"3.42.0\" }\n";
        let 文档 = 解析配置(原);
        let 输出 = 序列化配置文档(&文档);
        let 再解析 = 解析配置(&输出);
        let 段 = 取段(&再解析, "运行依赖").unwrap();
        let 表 = 取表(段, "数据库").unwrap();
        assert_eq!(表取文本(表, "原始名").as_deref(), Some("sqlite3"));
        assert_eq!(表取文本(表, "类型").as_deref(), Some("外部语言"));
    }

    #[test]
    fn bom_首段可读() {
        // 带 BOM 文件首行 \u{feff}[段名]：修复前整段被"容错忽略"吞掉，键值落入顶层
        let 文档 = 解析配置("\u{feff}[开发套件]\n开发套件版本 = \"0.0.2\"\n");
        assert!(文档.顶层.is_empty(), "BOM 段内容不应落入顶层");
        assert_eq!(文档.段.len(), 1);
        assert_eq!(文档.段[0].段名, "开发套件");
        assert_eq!(文档.段[0].键值[0].0, "开发套件版本");
        assert_eq!(文档.段[0].键值[0].1, 配置值::文本("0.0.2".into()));
    }

    #[test]
    fn bom_顶层键值对() {
        let 文档 = 解析配置("\u{feff}顶级 = \"v\"\n[段]\nk = 1");
        assert_eq!(文档.顶层.len(), 1);
        assert_eq!(文档.顶层[0].0, "顶级");
        assert_eq!(文档.顶层[0].1, 配置值::文本("v".into()));
        assert_eq!(文档.段.len(), 1);
        assert_eq!(文档.段[0].段名, "段");
    }
}
