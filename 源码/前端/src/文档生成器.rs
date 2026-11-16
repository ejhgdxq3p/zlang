// 文件: 前端/src/文档生成器.rs
// 职责: 智语言文档生成器 - 从源代码提取文档注释（//** **/）生成 Markdown API 文档

use crate::词法分析::{词条, 词法分析器, 词条类型};
use crate::语法分析::{语法分析器, 程序, 文档类别};
use crate::诊断收集器::诊断收集器;

pub struct 文档生成器<'a> {
    诊断: &'a mut 诊断收集器,
}

/// 文档节点类型
#[derive(Debug, Clone)]
pub enum 文档节点 {
    模块 { 名称: String, 描述: String },
    函数 { 名称: String, 描述: String, 形参文档: Vec<(String, String)>, 返回描述: String },
    结构体 { 名称: String, 描述: String, 字段文档: Vec<(String, String, String)> }, // (名称, 类型, 描述)
    枚举 { 名称: String, 描述: String, 成员文档: Vec<(String, String)> },
    变量 { 名称: String, 描述: String },
    接口 { 名称: String, 描述: String },
}

impl<'a> 文档生成器<'a> {
    pub fn 新建(诊断: &'a mut 诊断收集器) -> Self {
        文档生成器 { 诊断 }
    }

    /// 扫描源码文件提取所有文档注释节点
    ///
    /// 优先走 AST 关联（解析器已将 //** 注释挂到紧随其后的顶层定义），
    /// 自动覆盖无关键字函数、`公开`/`私有` 修饰、结构体/枚举/接口/变量/入口；
    /// 若 AST 方式未产出（如源码存在语法错误导致解析失败），回退到旧词法启发式。
    pub fn 提取文档(&mut self, 源码: &str, 文件路径: &str) -> Vec<文档节点> {
        let 词条列表 = 词法分析器::新建(源码, 文件路径.to_string(), self.诊断).扫描所有词条();
        let mut 解析器 = 语法分析器::新建(&词条列表, self.诊断);
        let 程序 = 解析器.解析程序();
        let 节点 = self.生成文档自程序(&程序);
        if !节点.is_empty() {
            return 节点;
        }
        自提取文档词法版(&词条列表)
    }

    /// 由 AST 文档列表生成 文档节点（遍历 程序.文档列表）
    fn 生成文档自程序(&self, 程序: &程序) -> Vec<文档节点> {
        let mut 文档列表 = Vec::new();
        for 条目 in &程序.文档列表 {
            let (描述, 形参文档, 返回描述) = 解析文档注释(&条目.文档);
            match 条目.类别 {
                文档类别::函数 | 文档类别::启动程序 => {
                    文档列表.push(文档节点::函数 {
                        名称: 条目.名称.clone(),
                        描述,
                        形参文档,
                        返回描述,
                    });
                }
                文档类别::结构体 => {
                    文档列表.push(文档节点::结构体 {
                        名称: 条目.名称.clone(),
                        描述,
                        字段文档: Vec::new(),
                    });
                }
                文档类别::枚举 => {
                    文档列表.push(文档节点::枚举 {
                        名称: 条目.名称.clone(),
                        描述,
                        成员文档: Vec::new(),
                    });
                }
                文档类别::接口 => {
                    文档列表.push(文档节点::接口 {
                        名称: 条目.名称.clone(),
                        描述,
                    });
                }
                文档类别::变量 | 文档类别::常量 => {
                    文档列表.push(文档节点::变量 {
                        名称: 条目.名称.clone(),
                        描述,
                    });
                }
            }
        }
        文档列表
    }

    /// 将文档节点列表渲染为 Markdown 文本
    pub fn 渲染标记文档(&self, 文档列表: &[文档节点], 模块名: &str) -> String {
        let mut 输出 = String::new();

        输出.push_str(&format!("# {} - API 文档\n\n", 模块名));
        输出.push_str("> 由智语言文档生成器自动生成\n\n");

        let mut 结构体节点 = Vec::new();
        let mut 枚举节点 = Vec::new();
        let mut 函数节点 = Vec::new();
        let mut 变量节点 = Vec::new();
        let mut 接口节点 = Vec::new();

        for 节点 in 文档列表 {
            match 节点 {
                文档节点::结构体 { .. } => 结构体节点.push(节点),
                文档节点::枚举 { .. } => 枚举节点.push(节点),
                文档节点::函数 { .. } => 函数节点.push(节点),
                文档节点::变量 { .. } => 变量节点.push(节点),
                文档节点::接口 { .. } => 接口节点.push(节点),
                文档节点::模块 { .. } => {}
            }
        }

        // 结构体列表
        if !结构体节点.is_empty() {
            输出.push_str("## 结构体\n\n");
            输出.push_str("| 名称 | 描述 |\n|------|------|\n");
            for 节点 in &结构体节点 {
                if let 文档节点::结构体 { 名称, 描述, .. } = 节点 {
                    输出.push_str(&format!("| {} | {} |\n", 名称, 描述));
                }
            }
            输出.push('\n');
        }

        // 枚举列表
        if !枚举节点.is_empty() {
            输出.push_str("## 枚举\n\n");
            输出.push_str("| 名称 | 描述 |\n|------|------|\n");
            for 节点 in &枚举节点 {
                if let 文档节点::枚举 { 名称, 描述, .. } = 节点 {
                    输出.push_str(&format!("| {} | {} |\n", 名称, 描述));
                }
            }
            输出.push('\n');
        }

        // 函数列表
        if !函数节点.is_empty() {
            输出.push_str("## 函数\n\n");
            for 节点 in &函数节点 {
                if let 文档节点::函数 { 名称, 描述, 形参文档, 返回描述 } = 节点 {
                    输出.push_str(&format!("### {}\n\n", 名称));
                    if !描述.is_empty() {
                        输出.push_str(&format!("{}\n\n", 描述));
                    }
                    if !形参文档.is_empty() {
                        输出.push_str("**参数**:\n\n");
                        输出.push_str("| 参数 | 描述 |\n|------|------|\n");
                        for (形参名, 形参描述) in 形参文档 {
                            输出.push_str(&format!("| {} | {} |\n", 形参名, 形参描述));
                        }
                        输出.push('\n');
                    }
                    if !返回描述.is_empty() {
                        输出.push_str(&format!("**返回**: {}\n\n", 返回描述));
                    }
                }
            }
        }

        // 接口列表
        if !接口节点.is_empty() {
            输出.push_str("## 接口\n\n");
            for 节点 in &接口节点 {
                if let 文档节点::接口 { 名称, 描述 } = 节点 {
                    输出.push_str(&format!("### {}\n\n", 名称));
                    if !描述.is_empty() {
                        输出.push_str(&format!("{}\n\n", 描述));
                    }
                }
            }
        }

        输出
    }
}

/// 解析文档注释内容：提取描述首段、`@param 名称 描述` 参数说明、`@return 描述` 返回说明。
/// 返回 (描述, 形参文档, 返回描述)。（规范 §3.1 / §3.5）
fn 解析文档注释(内容: &str) -> (String, Vec<(String, String)>, String) {
    let mut 描述行: Vec<&str> = Vec::new();
    let mut 形参文档: Vec<(String, String)> = Vec::new();
    let mut 返回描述 = String::new();
    for 行 in 内容.lines() {
        let 行 = 行.trim();
        if let Some(余) = 行.strip_prefix("@param") {
            let 余 = 余.trim();
            if let Some(空格处) = 余.find(char::is_whitespace) {
                let 名称 = 余[..空格处].to_string();
                let 描述 = 余[空格处..].trim().to_string();
                形参文档.push((名称, 描述));
            }
        } else if let Some(余) = 行.strip_prefix("@return") {
            返回描述 = 余.trim().to_string();
        } else if !行.is_empty() {
            描述行.push(行);
        }
    }
    (描述行.join("\n"), 形参文档, 返回描述)
}

/// 旧词法启发式 fallback：仅识别 `函数/结构体/枚举/变量` 关键字定义前的文档注释。
/// 当 AST 方式未能产出文档时（如源码存在语法错误）使用，作为兜底保证容错。
fn 自提取文档词法版(词条列表: &[词条]) -> Vec<文档节点> {
    let mut 文档列表 = Vec::new();
    let mut i = 0;
    let 词条数 = 词条列表.len();
    let mut 待定文档 = String::new();

    while i < 词条数 {
        let 词条 = &词条列表[i];

        if let 词条类型::文档注释内容(内容) = &词条.类型 {
            待定文档 = 内容.clone();
            i += 1;
            continue;
        }

        match &词条.类型 {
            词条类型::结构体 => {
                if i + 1 < 词条数 {
                    if let 词条类型::标识符(名称) = &词条列表[i + 1].类型 {
                        let (描述, _, _) = 解析文档注释(&待定文档);
                        文档列表.push(文档节点::结构体 {
                            名称: 名称.clone(),
                            描述,
                            字段文档: Vec::new(),
                        });
                        待定文档 = String::new();
                    }
                }
            }
            词条类型::枚举 => {
                if i + 1 < 词条数 {
                    if let 词条类型::标识符(名称) = &词条列表[i + 1].类型 {
                        let (描述, _, _) = 解析文档注释(&待定文档);
                        文档列表.push(文档节点::枚举 {
                            名称: 名称.clone(),
                            描述,
                            成员文档: Vec::new(),
                        });
                        待定文档 = String::new();
                    }
                }
            }
            词条类型::函数 => {
                if i + 1 < 词条数 {
                    if let 词条类型::标识符(名称) = &词条列表[i + 1].类型 {
                        let (描述, 形参文档, 返回描述) = 解析文档注释(&待定文档);
                        文档列表.push(文档节点::函数 {
                            名称: 名称.clone(),
                            描述,
                            形参文档,
                            返回描述,
                        });
                        待定文档 = String::new();
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    文档列表
}
