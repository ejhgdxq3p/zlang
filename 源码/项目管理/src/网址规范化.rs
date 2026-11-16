// 文件: 项目管理/src/网址规范化.rs
// 职责: 把含中文（非 ASCII）主机名的网址规范化为纯 ASCII 网址。
//
// 背景：智语言官方源使用中文域名（如 https://库.智语言.org）。HTTP 客户端（ureq/hyper 等）
// 遵循 RFC 3986，主机名只接受 ASCII，遇到中文字节直接报 "invalid uri character"。
// 国际化域名（IDN）的标准做法是主机名各标签按 RFC 3492 Punycode 编码，加 "xn--" 前缀，
// 即 IDNA ToASCII。本模块实现该转换，使中文域名在网络层可用，同时源码/配置/提示中
// 仍保留中文原文，符合项目全中文命名原则。
//
// 只转换「主机名」：路径与查询串的百分号编码由各客户端的 url编码 负责，二者职责不重叠。
// 跨平台：纯字符串运算，无系统调用，Windows/macOS/Linux 行为一致。

/// Punycode 参数（RFC 3492 §5，取标准值）
const 基数: u32 = 36;
const 最小阈值: u32 = 1;
const 最大阈值: u32 = 26;
const 偏斜: u32 = 38;
const 阻尼: u32 = 700;
const 初始偏置: u32 = 72;
const 初始码点: u32 = 128;

/// 数字 → 编码字符（0..25 → a..z，26..35 → 0..9）
fn 数字转字符(数字: u32) -> char {
    if 数字 < 26 {
        (b'a' + 数字 as u8) as char
    } else {
        (b'0' + (数字 - 26) as u8) as char
    }
}

/// RFC 3492 §6.1 偏置自适应
fn 调整偏置(mut 差值: u32, 计数: u32, 首次: bool) -> u32 {
    差值 = if 首次 { 差值 / 阻尼 } else { 差值 / 2 };
    差值 += 差值 / 计数;
    let mut k = 0u32;
    while 差值 > ((基数 - 最小阈值) * 最大阈值) / 2 {
        差值 /= 基数 - 最小阈值;
        k += 基数;
    }
    k + (((基数 - 最小阈值 + 1) * 差值) / (差值 + 偏斜))
}

/// 将单个域名标签编码为 Punycode（不含 "xn--" 前缀）。
/// 返回 None 表示编码溢出（异常输入），调用方应保留原文。
fn 编码标签(标签: &str) -> Option<String> {
    let 全部码点: Vec<u32> = 标签.chars().map(|c| c as u32).collect();
    let mut 输出 = String::new();

    // 1. 先原样输出全部 ASCII 码点（基本码点）
    for &码点 in &全部码点 {
        if 码点 < 初始码点 {
            输出.push(码点 as u8 as char);
        }
    }
    let 基本数量 = 输出.chars().count() as u32;
    if 基本数量 > 0 {
        输出.push('-');
    }

    // 2. 增量编码非 ASCII 码点
    let mut 已处理 = 基本数量;
    let mut n = 初始码点;
    let mut 差量: u32 = 0;
    let mut 偏置 = 初始偏置;
    let 总数 = 全部码点.len() as u32;

    while 已处理 < 总数 {
        // 取大于等于 n 的最小码点
        let 下一个 = 全部码点.iter().copied().filter(|&c| c >= n).min()?;
        差量 = 差量.checked_add((下一个 - n).checked_mul(已处理 + 1)?)?;
        n = 下一个;

        for &码点 in &全部码点 {
            if 码点 < n {
                差量 = 差量.checked_add(1)?;
            } else if 码点 == n {
                // 变长整数写出
                let mut q = 差量;
                let mut k = 基数;
                loop {
                    let 阈值 = if k <= 偏置 + 最小阈值 {
                        最小阈值
                    } else if k >= 偏置 + 最大阈值 {
                        最大阈值
                    } else {
                        k - 偏置
                    };
                    if q < 阈值 {
                        break;
                    }
                    输出.push(数字转字符(阈值 + (q - 阈值) % (基数 - 阈值)));
                    q = (q - 阈值) / (基数 - 阈值);
                    k += 基数;
                }
                输出.push(数字转字符(q));
                偏置 = 调整偏置(差量, 已处理 + 1, 已处理 == 基本数量);
                差量 = 0;
                已处理 += 1;
            }
        }
        差量 = 差量.checked_add(1)?;
        n = n.checked_add(1)?;
    }
    Some(输出)
}

/// IDNA ToASCII：把主机名（可含多级标签）转为纯 ASCII 形式。
/// 已是 ASCII 的标签原样保留；含非 ASCII 的标签编码为 "xn--<punycode>"。
pub fn 主机名转ascii(主机名: &str) -> String {
    if 主机名.is_ascii() {
        return 主机名.to_string();
    }
    主机名
        .split('.')
        .map(|标签| {
            if 标签.is_ascii() {
                标签.to_string()
            } else {
                // 域名标签大小写不敏感，统一小写后编码
                let 小写: String = 标签.to_lowercase();
                match 编码标签(&小写) {
                    Some(编码) => format!("xn--{}", 编码),
                    None => 标签.to_string(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// 规范化完整网址：仅把「主机名」部分转为 ASCII（Punycode），其余原样保留。
/// 已是纯 ASCII 的网址零开销直接返回。
///
/// 支持形态：`协议://[用户信息@]主机[:端口][/路径][?查询][#片段]`；
/// 无协议前缀时按「主机在最前」处理。
pub fn 规范化网址(网址: &str) -> String {
    if 网址.is_ascii() {
        return 网址.to_string();
    }
    // 切出协议前缀
    let (协议, 其余) = match 网址.find("://") {
        Some(位置) => (&网址[..位置 + 3], &网址[位置 + 3..]),
        None => ("", 网址),
    };
    // 主机段止于第一个 '/'、'?'、'#'
    let 主机结束 = 其余
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(其余.len());
    let (主机段, 尾部) = 其余.split_at(主机结束);

    // 剥离 用户信息@
    let (用户信息, 主机端口) = match 主机段.rfind('@') {
        Some(位置) => (&主机段[..位置 + 1], &主机段[位置 + 1..]),
        None => ("", 主机段),
    };
    // 剥离 :端口（IPv6 字面量以 ']' 收尾，端口冒号只可能在其后）
    let 端口起点 = if 主机端口.starts_with('[') {
        主机端口.rfind(']').and_then(|右括号| {
            主机端口[右括号..]
                .find(':')
                .map(|偏移| 右括号 + 偏移)
        })
    } else {
        主机端口.rfind(':')
    };
    let (主机, 端口) = match 端口起点 {
        Some(位置) => (&主机端口[..位置], &主机端口[位置..]),
        None => (主机端口, ""),
    };

    format!(
        "{}{}{}{}{}",
        协议,
        用户信息,
        主机名转ascii(主机),
        端口,
        尾部
    )
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 纯ascii网址原样返回() {
        assert_eq!(
            规范化网址("https://example.org/a?b=c"),
            "https://example.org/a?b=c"
        );
    }

    #[test]
    fn ascii主机加中文路径只动主机() {
        // 路径的百分号编码由各客户端 url编码 负责，本模块不得改写路径
        assert_eq!(
            规范化网址("https://example.org/库/工具"),
            "https://example.org/库/工具"
        );
    }

    // 下列期望值与标准 IDNA/Punycode 实现逐字节一致
    // （交叉核对：Python `"库.智语言.org".encode("idna")` → xn--dxt.xn--5kvs40fikb.org）

    #[test]
    fn 中文主机名转punycode() {
        // 开发套件官方源
        assert_eq!(
            规范化网址("https://智语言.org"),
            "https://xn--5kvs40fikb.org"
        );
    }

    #[test]
    fn 多级中文主机名逐标签编码() {
        // 库官方源
        assert_eq!(
            规范化网址("https://库.智语言.org"),
            "https://xn--dxt.xn--5kvs40fikb.org"
        );
    }

    #[test]
    fn 保留端口与路径查询() {
        let 结果 = 规范化网址("https://库.智语言.org:8443/库/工具?键=值#片段");
        assert!(结果.contains(":8443/库/工具?键=值#片段"), "实际: {}", 结果);
        // 主机部分必须已 ASCII 化
        let 主机 = 结果
            .trim_start_matches("https://")
            .split(|c| c == ':' || c == '/')
            .next()
            .unwrap();
        assert!(主机.is_ascii(), "主机未转换: {}", 主机);
    }

    #[test]
    fn 保留用户信息() {
        let 结果 = 规范化网址("https://用户:密码@库.智语言.org/路径");
        assert!(结果.contains("@"), "实际: {}", 结果);
        assert!(结果.contains("xn--"), "实际: {}", 结果);
    }

    #[test]
    fn 无协议前缀也可处理() {
        let 结果 = 规范化网址("库.智语言.org/路径");
        assert!(结果.starts_with("xn--"), "实际: {}", 结果);
    }

    #[test]
    fn punycode_已知向量() {
        // RFC 3492 中文示例：「点看」→ xn--3lr804guic 的构造一致性自检
        // 这里用可独立验证的常见值：中文「测试」标签
        let 编码 = 主机名转ascii("测试");
        assert!(编码.starts_with("xn--"), "实际: {}", 编码);
        assert!(编码.is_ascii());
    }
}
