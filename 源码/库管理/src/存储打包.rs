// 文件: 库管理/src/存储打包.rs
// 职责: 纯 Rust ZIP 构造（stored 方法，与 zip crate 兼容），供库封装/工具包发布共用
//   注意：ZIP 解压逻辑在 版本管理器/src/开发套件安装器.rs（开发套件安装器），本文件只负责构造。

// 将文件列表构造为 stored 方法的 ZIP 字节流
pub fn 构造存储zip(文件列表: &[(&str, &[u8])]) -> Vec<u8> {
    let 计算crc32 = |数据: &[u8]| -> u32 {
        let mut 表 = [0u32; 256];
        for 序号 in 0..256u32 { let mut 值 = 序号; for _ in 0..8 { 值 = if 值&1!=0 {0xedb88320^(值>>1)} else {值>>1}; } 表[序号 as usize]=值; }
        let mut 校验值: u32 = 0xffffffff;
        for &字节 in 数据 { 校验值 = 表[((校验值^字节 as u32)&0xff) as usize]^(校验值>>8); }
        校验值 ^ 0xffffffff
    };
    let mut 输出 = Vec::new(); let mut 中央 = Vec::new(); let mut 偏移 = 0u32;
    for (名, 内容) in 文件列表 {
        let 名b = 名.as_bytes(); let 校验 = 计算crc32(内容); let 大小 = 内容.len() as u32;
        let mut 头 = Vec::new();
        头.extend(b"PK\x03\x04"); 头.extend(&20u16.to_le_bytes()); 头.extend(&0x0800u16.to_le_bytes());
        头.extend(&0u16.to_le_bytes()); 头.extend(&0u16.to_le_bytes()); 头.extend(&0u16.to_le_bytes());
        头.extend(&校验.to_le_bytes()); 头.extend(&大小.to_le_bytes()); 头.extend(&大小.to_le_bytes());
        头.extend(&(名b.len() as u16).to_le_bytes()); 头.extend(&0u16.to_le_bytes());
        头.extend(名b); 头.extend(*内容);
        let 本地偏移 = 偏移; 偏移 += 头.len() as u32; 输出.extend(&头);
        let mut 中 = Vec::new();
        中.extend(b"PK\x01\x02"); 中.extend(&20u16.to_le_bytes()); 中.extend(&20u16.to_le_bytes());
        中.extend(&0x0800u16.to_le_bytes()); 中.extend(&0u16.to_le_bytes()); 中.extend(&0u16.to_le_bytes());
        中.extend(&0u16.to_le_bytes()); 中.extend(&校验.to_le_bytes()); 中.extend(&大小.to_le_bytes());
        中.extend(&大小.to_le_bytes()); 中.extend(&(名b.len() as u16).to_le_bytes());
        中.extend(&0u16.to_le_bytes()); 中.extend(&0u16.to_le_bytes()); 中.extend(&0u16.to_le_bytes());
        中.extend(&0u16.to_le_bytes()); 中.extend(&0u32.to_le_bytes()); 中.extend(&本地偏移.to_le_bytes());
        中.extend(名b); 中央.extend(&中);
    }
    let cd偏移 = 偏移; let cd大小 = 中央.len() as u32; let 条目数 = 文件列表.len() as u16;
    输出.extend(&中央);
    let mut 结束记录 = Vec::new();
    结束记录.extend(b"PK\x05\x06"); 结束记录.extend(&0u16.to_le_bytes()); 结束记录.extend(&0u16.to_le_bytes());
    结束记录.extend(&条目数.to_le_bytes()); 结束记录.extend(&条目数.to_le_bytes());
    结束记录.extend(&cd大小.to_le_bytes()); 结束记录.extend(&cd偏移.to_le_bytes());
    结束记录.extend(&0u16.to_le_bytes()); 输出.extend(&结束记录);
    输出
}
