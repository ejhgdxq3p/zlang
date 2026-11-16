// 文件: 标准库功能/工具系统函数.rs
// 职责: 工具 标准库 native 后端（与 标准库/工具.智 门面对应）。
//       由 分发器.rs::系统调用() 的 35..=41 分支委托到 处理工具()。
//       正则缓存 定义于 分发器.rs（祖先模块私有项，子模块可访问），
//       经 super::正则缓存 引用，保持全局缓存单例。
//       Base64 编解码定义于 文本系统函数（同层后端），经 super::文本系统函数 引用。

use crate::标准库索引;
use crate::值类型::*;
use super::{基础辅助::取文本, 文本系统函数::{Base64编码, Base64解码}};
use regex::Regex;

pub(crate) fn 处理工具(索引: usize, 参数: &[运行时值]) -> 运行时值 {
    match 索引 {
        标准库索引::工具_唯一编号 => {
            let s = uuid::Uuid::new_v4().to_string();
            分配文本(s)
        }
        标准库索引::工具_正则匹配 => {
            let 模式 = 取文本(参数, 0);
            let 内容 = 取文本(参数, 1);
            let mut 缓存 = super::正则缓存.lock().unwrap();
            let 结果 = if let Some(re) = 缓存.get(&模式) {
                re.find_iter(&内容).map(|m| 分配文本(m.as_str().to_string())).collect()
            } else if let Ok(re) = Regex::new(&模式) {
                let matches: Vec<运行时值> = re.find_iter(&内容).map(|m| 分配文本(m.as_str().to_string())).collect();
                缓存.insert(模式, re);
                matches
            } else {
                // ⚡双模式⚡: 非法正则语法 → 抛“正则语法错误”可捕获异常（此前静默返回空数组，掩盖错误）。
                let 错误 = format!("正则表达式语法错误: {}", 模式);
                crate::异常运行时::智语言_抛出异常(
                    "运行时异常".as_ptr(), "运行时异常".len() as i64,
                    错误.as_ptr(), 错误.len() as i64,
                );
                vec![]
            };
            分配数组(结果)
        }
        标准库索引::工具_哈希算法 => {
            // 规范 5.6.3：工具.哈希算法(文本 算法, 文本 数据)
            //   算法 ∈ {"MD5", "SHA256"}，返回对应哈希的小写十六进制串。
            let 算法 = 取文本(参数, 0);
            let 数据 = 取文本(参数, 1);
            let 字节 = 数据.as_bytes();
            // ★ 2026-09-27（**F14 修复**）不支持的算法名 ⇒ **抛可捕获异常**（原为静默返回错误文本 ✗）。
            //   原实现 `_ => format!("不支持的哈希算法：{}", 算法)` **把错误说明当成哈希值返回**：
            //   实测 `工具.哈希算法("SHA1", "abc")` 得到 15 个字符的错误文本 ——
            //   长度既非 32（MD5）亦非 64（SHA256），调用方若不校验长度就会把它当作合法哈希继续使用 ✗。
            //   依据：① 本文件上方「正则表达式语法错误」分支已是同一范式（其注释明写"此前静默返回空数组，掩盖错误"）；
            //        ② 本项目约定「禁静默返回空值/假值，避免缺陷被吞掉」。
            let s = match 算法.as_str() {
                "MD5" => md5摘要(字节),
                "SHA256" => sha256摘要(字节),
                _ => {
                    抛工具参数错误(&format!(
                        "不支持的哈希算法：{}（仅支持 MD5、SHA256）", 算法
                    ));
                    String::new()
                }
            };
            分配文本(s)
        }
        标准库索引::工具_编码方案 => {
            let 方案 = 取文本(参数, 0);
            let 数据 = 取文本(参数, 1);
            // ★ 2026-09-27（**F14 修复**）不支持方案 ⇒ 抛异常（原 `else { 数据 }` **原样返回输入** ✗）：
            //   实测 `工具.编码方案("HEX", "甲乙")` 返回 `甲乙`，调用方会误以为"已编码"继续使用 ✗。
            let s = if 方案 == "BASE64" {
                Base64编码(&数据)
            } else {
                抛工具参数错误(&format!(
                    "不支持的编码方案：{}（仅支持 BASE64）", 方案
                ));
                String::new()
            };
            分配文本(s)
        }
        标准库索引::工具_解码方案 => {
            let 方案 = 取文本(参数, 0);
            let 数据 = 取文本(参数, 1);
            // ★ 2026-09-27（**F14 修复**）：同「编码方案」——不支持方案原样返回输入 ✗ ⇒ 改为抛异常。
            // ★ 2026-10-19（核心库全面测试）：**非法 BASE64 数据**同属非法输入 —— 原实现静默跳过
            //   非法字符并返回空串（`解码方案("BASE64","!!!!")` ⇒ ""），调用方会把空串当合法结果 ✗。
            //   `Base64解码` 已改为返回 Result，此处统一抛可捕获异常。
            let s = if 方案 == "BASE64" {
                match Base64解码(&数据) {
                    Ok(文本) => 文本,
                    Err(说明) => {
                        抛工具参数错误(&说明);
                        String::new()
                    }
                }
            } else {
                抛工具参数错误(&format!(
                    "不支持的解码方案：{}（仅支持 BASE64）", 方案
                ));
                String::new()
            };
            分配文本(s)
        }
        _ => 运行时值::空值(),
    }
}

/// ★ 2026-09-27（**F14**）：工具模块**参数取值非法时抛可捕获异常**的统一出口。
///
/// **为什么必须抛而不能返回**：本模块三个函数原先对非法参数分别采取
/// 「返回错误文本当结果」「原样返回输入」的静默处置 ⇒ **缺陷被吞掉**：
///   · `工具.哈希算法("SHA1", 数据)` ⇒ 返回字符串 `不支持的哈希算法：SHA1`（被当成哈希）
///   · `工具.编码方案("HEX", 数据)` / `解码方案("HEX", 数据)` ⇒ 返回 `数据` 本身（被当成已编码/已解码）
/// 调用方无从察觉，最终产出静默错误的业务数据 ✗。
/// 依据：① 本文件 `工具.正则匹配` 的非法模式**已是抛出**（同范式，注释即"此前静默返回空数组，掩盖错误"）；
///      ② 本项目约定「禁静默返回空值，避免缺陷被吞掉」（见 `堆操作.rs::抛不支持容器方法`）；
///      ③ 规范 §5.6.3/§5.6.4 只定义 MD5/SHA256 与 BASE64 ⇒ 其余取值属**未定义输入**，抛异常是唯一无歧义处置。
/// 抛出的异常可被 `捕获:` 捕获，`异常错误().描述` 即本函数传入的说明 ✓。
fn 抛工具参数错误(描述: &str) {
    crate::异常运行时::智语言_抛出异常(
        "运行时异常".as_ptr(), "运行时异常".len() as i64,
        描述.as_ptr(), 描述.len() as i64,
    );
}

/// MD5（RFC 1321）纯 Rust 实现，返回小写十六进制串。
fn md5摘要(输入: &[u8]) -> String {
    const S: [u32; 64] = [
        7,12,17,22, 7,12,17,22, 7,12,17,22, 7,12,17,22,
        5,9,14,20, 5,9,14,20, 5,9,14,20, 5,9,14,20,
        4,11,16,23, 4,11,16,23, 4,11,16,23, 4,11,16,23,
        6,10,15,21, 6,10,15,21, 6,10,15,21, 6,10,15,21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,
        0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
        0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,
        0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,
        0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    let mut 消息 = 输入.to_vec();
    let 原长位 = (输入.len() as u64).wrapping_mul(8);
    消息.push(0x80);
    while 消息.len() % 64 != 56 {
        消息.push(0);
    }
    消息.extend_from_slice(&原长位.to_le_bytes());

    for 块 in 消息.chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([块[i*4], 块[i*4+1], 块[i*4+2], 块[i*4+3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | (!b & d), i)
            } else if i < 32 {
                ((d & b) | (!d & c), (5*i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3*i + 5) % 16)
            } else {
                (c ^ (b | !d), (7*i) % 16)
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            let f = f.rotate_left(S[i]).wrapping_add(b);
            a = d; d = c; c = b; b = f;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut 输出 = [0u8; 16];
    输出[0..4].copy_from_slice(&a0.to_le_bytes());
    输出[4..8].copy_from_slice(&b0.to_le_bytes());
    输出[8..12].copy_from_slice(&c0.to_le_bytes());
    输出[12..16].copy_from_slice(&d0.to_le_bytes());
    输出.iter().map(|x| format!("{:02x}", x)).collect()
}

/// SHA256（FIPS 180-4）纯 Rust 实现，返回小写十六进制串。
fn sha256摘要(输入: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let mut 消息 = 输入.to_vec();
    let 原长位 = (输入.len() as u64).wrapping_mul(8);
    消息.push(0x80);
    while 消息.len() % 64 != 56 {
        消息.push(0);
    }
    消息.extend_from_slice(&原长位.to_be_bytes());

    for 块 in 消息.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([块[i*4], 块[i*4+1], 块[i*4+2], 块[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (
            h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7],
        );
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1); d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut 输出 = Vec::with_capacity(32);
    for x in h {
        输出.extend_from_slice(&x.to_be_bytes());
    }
    输出.iter().map(|x| format!("{:02x}", x)).collect()
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn md5已知向量() {
        assert_eq!(md5摘要(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5摘要(b"hello world"), "5eb63bbbe01eeed093cb22bb8f5acdc3");
        assert_eq!(
            md5摘要(b"The quick brown fox jumps over the lazy dog"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
    }

    #[test]
    fn sha256已知向量() {
        assert_eq!(
            sha256摘要(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256摘要(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert_eq!(
            sha256摘要(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }
}
