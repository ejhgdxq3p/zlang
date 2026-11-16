// 文件: 项目管理/src/命名规范.rs
// 职责: 规范 §3.7「中文命名规范」的**字符集真源** + **路径级强制检查**（§3.7.4 编译器强制检查）
//
// 双模式说明：本模块只做静态检查，**不产生 LIR、不进入任何后端**。
//   解释模式与编译模式共用同一份前端/工程层判定 ⇒ 双模天然一致，不存在 parity 风险。
//
// 为什么放在「项目管理」crate：
//   `前端`（词法分析复用字符集、语义分析器做项目扫描）、`版本管理器`（4 条命令做单文件路径前置校验）
//   都依赖本 crate，而 `项目管理` 不依赖它们 ⇒ 这里是唯一的公共下限，避免字符集出现第二份实现。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 豁免子树：工具生成物 / 第三方资源，**不属于智语言命名空间**，不参与 §3.7 路径校验。
///
/// 依据：`外部语言` 是 FFI 引用的 C/C++/GTK 等第三方目录（其命名不由本语言决定）；
/// `产出` / `日志` / `文件` 是本工具链生成物；`库` 是已安装第三方库（各自为自己的项目，独立校验）。
const 豁免目录: &[&str] = &[
    "产出",
    "日志",
    "文件",
    "外部语言",
    "库",
    "node_modules",
    ".git",
    "target",
];

/// 判断字符是否为中文简繁汉字（含 CJK 统一汉字及扩展 A-H 区，以及兼容汉字）。
///
/// 这是**全语言唯一的字符集真源**（规范 §3.7.1/§3.7.2）：
/// 词法分析（标识符级）与路径校验（文件/目录级）都必须调用本函数，禁止各自再写一份。
pub fn 是中文汉字(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'    // CJK统一汉字（常用简繁汉字）
        | '\u{3400}'..='\u{4DBF}'  // CJK扩展A
        | '\u{20000}'..='\u{2A6DF}' // CJK扩展B
        | '\u{2A700}'..='\u{2B73F}' // CJK扩展C
        | '\u{2B740}'..='\u{2B81F}' // CJK扩展D
        | '\u{2B820}'..='\u{2CEAF}' // CJK扩展E
        | '\u{2CEB0}'..='\u{2EBEF}' // CJK扩展F
        | '\u{30000}'..='\u{3134F}' // CJK扩展G
        | '\u{31350}'..='\u{323AF}' // CJK扩展H
        | '\u{F900}'..='\u{FAFF}'   // CJK兼容汉字
        | '\u{2F800}'..='\u{2FA1F}' // CJK兼容补充
    )
}

/// 文件/目录名允许的字符集：中文简繁汉字 + 半角数字 + 半角下划线 + 半角点（规范 §3.7.2）。
pub fn 是合法名称字符(ch: char) -> bool {
    是中文汉字(ch) || ch.is_ascii_digit() || ch == '_' || ch == '.'
}

/// 非法字符的中文说明（用于诊断文案，与 §3.6.4 全角标点专门检测口径一致）。
pub fn 非法字符说明(ch: char) -> &'static str {
    if ch.is_ascii_alphabetic() {
        "英文字母"
    } else if ch == ' ' {
        "空格"
    } else if ch == '-' {
        "连字符"
    } else if ch.is_ascii() {
        "半角符号"
    } else if ('\u{FF01}'..='\u{FF5E}').contains(&ch) || ch == '\u{3000}' {
        "全角标点符号"
    } else if ch.is_alphabetic() {
        "非汉字字母"
    } else {
        "非汉字字符"
    }
}

/// 一条路径命名违规。
#[derive(Debug, Clone)]
pub struct 命名违规 {
    /// 违规对象相对项目根（或单文件模式下的文件名）的路径，用于诊断定位
    pub 位置: String,
    /// 违规的那一段名称
    pub 段: String,
    /// 首个非法字符
    pub 非法字符: char,
    /// 非法字符说明
    pub 说明: &'static str,
    /// true=目录，false=文件
    pub 是目录: bool,
}

impl 命名违规 {
    /// 规范 §3.7.4 格式的诊断描述。
    pub fn 描述(&self) -> String {
        let 名类 = if self.是目录 { "文件夹名" } else { "文件名" };
        format!(
            "路径不合法：【{}】中【{}】含{}，{}只能使用中文简繁汉字、半角下划线、半角数字、半角点",
            self.位置, self.段, self.说明, 名类
        )
    }

    /// 修复建议。
    pub fn 帮助(&self) -> String {
        if self.是目录 {
            "请将文件夹改为中文名称，如\"源码\"".to_string()
        } else {
            "请将文件改为中文名称，如\"启动程序.智\"".to_string()
        }
    }
}

/// 检查单个名称，返回首个非法字符及其说明。
fn 检查名称(名称: &str) -> Option<(char, &'static str)> {
    for ch in 名称.chars() {
        if !是合法名称字符(ch) {
            return Some((ch, 非法字符说明(ch)));
        }
    }
    None
}

fn 是豁免目录(名: &str) -> bool {
    豁免目录.contains(&名)
}

/// 递归扫描项目目录，校验所有**目录名**与 **`.智` / `.配置` 文件名**（规范 §3.7.4「项目目录及文件」）。
///
/// 说明：只校验智语言命名空间内的对象——其余普通文件（`.md`/`.rs`/`.txt`/二进制等）不参与，
/// 因为它们的命名由各自生态决定（如 `README.md`、`Cargo.toml`）。
pub fn 检查项目目录(项目根: &Path) -> Vec<命名违规> {
    let mut 结果 = Vec::new();
    递归检查(项目根, 项目根, &mut 结果);
    // 稳定输出：按位置排序，避免目录遍历顺序导致诊断顺序抖动（§3.6.6 多错误按行号升序的同类要求）
    结果.sort_by(|甲, 乙| 甲.位置.cmp(&乙.位置));
    结果
}

fn 递归检查(项目根: &Path, 当前: &Path, 结果: &mut Vec<命名违规>) {
    let 目录项 = match std::fs::read_dir(当前) {
        Ok(v) => v,
        Err(_) => return,
    };
    for 项 in 目录项.flatten() {
        let 路径 = 项.path();
        let 名 = 项.file_name().to_string_lossy().to_string();
        let 文件类型 = match 项.file_type() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if 文件类型.is_dir() {
            if 是豁免目录(&名) {
                continue;
            }
            if let Some((ch, 说明)) = 检查名称(&名) {
                结果.push(命名违规 {
                    位置: 相对显示(路径.strip_prefix(项目根).unwrap_or(&路径)),
                    段: 名.clone(),
                    非法字符: ch,
                    说明,
                    是目录: true,
                });
            }
            递归检查(项目根, &路径, 结果);
        } else if 文件类型.is_file() {
            // 只校验智语言源文件与配置文件（§3.7.2 适用范围）
            let 后缀 = 路径.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            if 后缀 != "智" && 后缀 != "配置" {
                continue;
            }
            if let Some((ch, 说明)) = 检查名称(&名) {
                结果.push(命名违规 {
                    位置: 相对显示(路径.strip_prefix(项目根).unwrap_or(&路径)),
                    段: 名,
                    非法字符: ch,
                    说明,
                    是目录: false,
                });
            }
        }
    }
}

/// 单文件路径前置校验（规范 §3.7.4「单文件路径」）。
///
/// 口径（重要，避免把用户环境路径误判）：
/// - **文件属于某个项目** ⇒ 校验「相对项目根」的**每一段**（目录 + 文件名）；
/// - **文件不属于任何项目** ⇒ 只校验**文件名本身**（所在目录属于用户环境，如 `D:\code\...`，不在语言管辖范围）。
pub fn 检查入口文件路径(文件路径: &Path) -> Vec<命名违规> {
    let mut 结果 = Vec::new();
    let 显示路径 = 文件路径.display().to_string();

    if let Some(项目根) = crate::依赖解析::检测项目根(文件路径) {
        if let Ok(相对) = 文件路径.strip_prefix(&项目根) {
            let 段列表: Vec<String> = 相对
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            for (序号, 段) in 段列表.iter().enumerate() {
                if 序号 < 段列表.len() - 1 && 是豁免目录(段) {
                    break;
                }
                if let Some((ch, 说明)) = 检查名称(段) {
                    结果.push(命名违规 {
                        位置: 相对显示(相对),
                        段: 段.clone(),
                        非法字符: ch,
                        说明,
                        是目录: 序号 < 段列表.len() - 1,
                    });
                    break; // 同一路径只报首个非法段，避免级联噪声
                }
            }
            return 结果;
        }
    }

    // 项目外（或无法相对化）：只校验文件名
    if let Some(名) = 文件路径.file_name().map(|v| v.to_string_lossy().to_string()) {
        if let Some((ch, 说明)) = 检查名称(&名) {
            结果.push(命名违规 {
                位置: 显示路径,
                段: 名,
                非法字符: ch,
                说明,
                是目录: false,
            });
        }
    }
    结果
}

/// 带进程级缓存的项目目录扫描（同一项目在一个进程内只完整扫一次）。
///
/// 动机：`测试` / `检查` 命令会在**同一进程内串行分析多个 `.智` 文件**，
/// 若每个文件都递归扫描整个项目，开销随文件数线性放大；而无违规是绝大多数情况，缓存收益显著。
/// 有违规时**不缓存**（每次都重扫），保证修复后立刻重新报出。
pub fn 检查项目目录_缓存(项目根: &Path) -> Vec<命名违规> {
    static 已通过: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    let 表 = 已通过.get_or_init(|| Mutex::new(HashSet::new()));
    let 键 = std::fs::canonicalize(项目根).unwrap_or_else(|_| 项目根.to_path_buf());
    if let Ok(集合) = 表.lock() {
        if 集合.contains(&键) {
            return Vec::new();
        }
    }
    let 结果 = 检查项目目录(项目根);
    if 结果.is_empty() {
        if let Ok(mut 集合) = 表.lock() {
            集合.insert(键);
        }
    }
    结果
}

fn 相对显示(p: &Path) -> String {
    // 规范 §3.7.4 示例使用 `/` 分隔（`src/主程序.智`），故统一用 `/` 渲染，跨平台一致
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}
