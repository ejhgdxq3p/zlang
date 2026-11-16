// 校验.rs - 纯Rust SHA-256（结果与 sha2 crate 完全一致，NIST FIPS 180-4）
use std::path::Path;

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

fn 右旋(x: u32, n: u32) -> u32 { (x >> n) | (x << (32 - n)) }

/// 计算字节数组的 SHA-256 摘要（小写十六进制，与 sha2 crate 结果逐比特一致）
pub fn 计算sha256(数据: &[u8]) -> String {
    let mut 哈希值 = [0x6a09e667u32,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
    let 位长 = (数据.len() as u64) * 8;
    let mut 消息 = 数据.to_vec();
    消息.push(0x80);
    while 消息.len() % 64 != 56 { 消息.push(0); }
    消息.extend_from_slice(&位长.to_be_bytes());
    for 块 in 消息.chunks(64) {
        let mut 消息调度 = [0u32; 64];
        for i in 0..16 { 消息调度[i] = u32::from_be_bytes([块[i*4],块[i*4+1],块[i*4+2],块[i*4+3]]); }
        for i in 16..64 {
            let s0 = 右旋(消息调度[i-15],7) ^ 右旋(消息调度[i-15],18) ^ (消息调度[i-15]>>3);
            let s1 = 右旋(消息调度[i-2],17) ^ 右旋(消息调度[i-2],19) ^ (消息调度[i-2]>>10);
            消息调度[i] = 消息调度[i-16].wrapping_add(s0).wrapping_add(消息调度[i-7]).wrapping_add(s1);
        }
        let (mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut hh) = (哈希值[0],哈希值[1],哈希值[2],哈希值[3],哈希值[4],哈希值[5],哈希值[6],哈希值[7]);
        for i in 0..64 {
            let s1 = 右旋(e,6) ^ 右旋(e,11) ^ 右旋(e,25);
            let ch = (e&f) ^ ((!e)&g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(消息调度[i]);
            let s0 = 右旋(a,2) ^ 右旋(a,13) ^ 右旋(a,22);
            let maj = (a&b) ^ (a&c) ^ (b&c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        哈希值[0]=哈希值[0].wrapping_add(a); 哈希值[1]=哈希值[1].wrapping_add(b); 哈希值[2]=哈希值[2].wrapping_add(c); 哈希值[3]=哈希值[3].wrapping_add(d);
        哈希值[4]=哈希值[4].wrapping_add(e); 哈希值[5]=哈希值[5].wrapping_add(f); 哈希值[6]=哈希值[6].wrapping_add(g); 哈希值[7]=哈希值[7].wrapping_add(hh);
    }
    哈希值.iter().map(|x| format!("{:08x}",x)).collect()
}

/// 校验 ZIP 文件的 SHA-256（从 .sha256 文件读取期望值）
pub fn 校验sha256文件(zip: &Path, sha文件: &Path) -> Result<(), String> {
    if !sha文件.exists() { return Ok(()); }
    let 期望 = std::fs::read_to_string(sha文件).map_err(|e| format!("读取校验文件失败: {}", e))?;
    // BOM 容错：U+FEFF 非空白字符，trim 剥不掉；带 BOM 的 .sha256 文件会与计算值比较失败
    校验sha256文本(zip, 期望.trim_start_matches('\u{feff}').trim())
}

/// 校验 ZIP 文件的 SHA-256（直接传入期望的十六进制摘要）
pub fn 校验sha256文本(zip: &Path, 期望: &str) -> Result<(), String> {
    if 期望.is_empty() { return Ok(()); }
    let 数据 = std::fs::read(zip).map_err(|e| format!("读取ZIP失败: {}", e))?;
    let 实际 = 计算sha256(&数据);
    if 实际.eq_ignore_ascii_case(期望) {
        eprintln!("[智语言] 校验通过");
        Ok(())
    } else {
        Err(format!("SHA256校验失败\n  期望: {}\n  实际: {}", 期望, 实际))
    }
}

#[cfg(test)]
mod 测试 {
    use super::*;
    #[test] fn sha256_空() { assert_eq!(计算sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"); }
    #[test] fn sha256_abc() { assert_eq!(计算sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"); }

    #[test]
    fn bom_期望值剥离() {
        // 带 BOM 的 .sha256 期望值文件应能校验通过（修复前 BOM 残留导致比较失败）
        let 目录 = std::env::temp_dir().join(format!("校验bom测试_{}", std::process::id()));
        std::fs::create_dir_all(&目录).unwrap();
        let zip路径 = 目录.join("测试.bin");
        let sha路径 = 目录.join("测试.bin.sha256");
        std::fs::write(&zip路径, b"abc").unwrap();
        let 哈希 = 计算sha256(b"abc");
        std::fs::write(&sha路径, format!("\u{feff}{}\n", 哈希)).unwrap();
        let 结果 = 校验sha256文件(&zip路径, &sha路径);
        std::fs::remove_dir_all(&目录).ok();
        assert!(结果.is_ok(), "带 BOM 期望值应校验通过: {:?}", 结果.err());
    }
}
