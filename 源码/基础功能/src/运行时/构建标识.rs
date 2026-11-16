// 文件: 基础功能/src/运行时/构建标识.rs
// 职责: 把「本运行时产物由哪份源码构建」写进产物自身，并提供从产物中读出该标识的唯一实现。
//
// 背景（2026-11-03，根治「外部标记文件与产物脱节」）：
//   此前 base.lib 与工具是否同源，靠 产出/release/.base_build_fingerprint 这个外部标记文件记录。
//   该文件只由 `链接器::构建运行时静态库` 在重建成功后写 ⇒ 任何人直接
//   `cargo build -p basic_functions` 都会得到「新库 + 旧标记」：
//     · 打包闸门假失败（拒绝打包一个其实正确的库）；
//     · 便携机（无源码/无 cargo）上判定为过期 ⇒ 尝试重建 → 必然失败 → 每次编译打警告。
//   现在身份随产物走：谁构建的库，库里就写着谁。打包器与链接器都从这里读，
//   不再存在「需要有人记得更新」的第二份记录。
//
// 落点说明：标识是 `.rdata` 里的普通字符串常量，在 base.lib / libbase.a / 用户 exe 里都能被
//   原样搜到（COFF / ELF / Mach-O 归档都不压缩对象文件），故无需链接即可校验。

use std::path::Path;

/// 标识前缀（与 版本管理器/src/构建签名.rs 的 `构建标识文本` 必须逐字一致）。
pub const 标识前缀: &str = "ZHI-BASE-FP:";

/// 指纹长度（`{:016x}` 恒为 16 位）。
const 指纹长度: usize = 16;

/// 内嵌构建标识：`ZHI-BASE-FP:<16 位十六进制>`，末尾 `\0` 便于 C 侧直接当 C 字符串用。
///
/// ⚠ 这里用 `env!` 而非 `option_env!`：`基础指纹` 由 build.rs 注入；若有人删掉 build.rs，
/// 编译期就会明确失败（比"悄悄产出一个没有身份的库"安全得多）。
pub const 构建标识文本: &str = concat!("ZHI-BASE-FP:", env!("基础指纹"), "\0");

/// 返回本产物的构建指纹（不含前缀与结尾 `\0`）。
pub fn 构建标识() -> &'static str {
    // 从同一份字面量派生：保证「字面量一定被使用」⇒ 一定落进只读数据段 ⇒ 一定能被扫到。
    构建标识文本
        .strip_prefix(标识前缀)
        .and_then(|余| 余.strip_suffix('\0'))
        .unwrap_or(构建标识文本)
}

/// C ABI：返回内嵌构建标识（完整 `ZHI-BASE-FP:<hex>\0`）。`长度` 非空时写入**不含结尾 `\0`** 的长度。
///
/// 对外稳定契约：工具链与外部校验脚本可据此确认「这份静态库是谁构建的」。
#[export_name = "智语言_构建标识"]
pub extern "C" fn 智语言_构建标识(长度: *mut i64) -> *const u8 {
    let 字节 = 构建标识文本.as_bytes();
    if !长度.is_null() {
        unsafe {
            *长度 = 字节.len().saturating_sub(1) as i64;
        }
    }
    字节.as_ptr()
}

/// 从任意产物文件（静态库 / 可执行文件）中读出内嵌构建指纹。
///
/// 语义（失败即安全）：
///   · 恰好 1 个标识 → `Ok(指纹)`；
///   · 0 个 → `Err`（旧版产物或非本工具链产物，校验方应拒绝）；
///   · 出现多个**不同**标识 → `Err`（疑似拼接了不同批次对象，属真实异常）。
pub fn 读取内嵌标识(路径: &Path) -> Result<String, String> {
    let 字节 = std::fs::read(路径)
        .map_err(|错误| format!("读取 {} 失败: {}", 路径.display(), 错误))?;
    let 前缀 = 标识前缀.as_bytes();
    let mut 命中: Vec<String> = Vec::new();
    let mut 起点 = 0usize;
    while let Some(偏移) = 查找(&字节[起点..], 前缀) {
        let 数值起 = 起点 + 偏移 + 前缀.len();
        let 数值止 = 数值起 + 指纹长度;
        if 数值止 <= 字节.len() {
            let 段 = &字节[数值起..数值止];
            if 段.iter().all(|b| b.is_ascii_hexdigit()) {
                let 指纹 = String::from_utf8_lossy(段).to_ascii_lowercase();
                if !命中.contains(&指纹) {
                    命中.push(指纹);
                }
            }
        }
        起点 += 偏移 + 前缀.len();
    }
    match 命中.len() {
        0 => Err("产物内不含构建标识（可能是旧版构建产物，或由其它工具链构建）".to_string()),
        1 => Ok(命中.remove(0)),
        _ => Err(format!(
            "产物内含多个不同的构建标识（{:?}），疑似拼接了不同批次的对象，拒绝使用",
            命中
        )),
    }
}

/// 朴素的子串查找（首字节过滤 + 逐字节比较）。
/// 不引入额外依赖；在 40MB 量级产物上用于「每次打包 / 每次重建判定」，耗时可接受。
fn 查找(堆: &[u8], 针: &[u8]) -> Option<usize> {
    if 针.is_empty() || 堆.len() < 针.len() {
        return None;
    }
    let 首 = 针[0];
    let 尾界 = 堆.len() - 针.len();
    let mut 起 = 0usize;
    while 起 <= 尾界 {
        match 堆[起..].iter().position(|&b| b == 首) {
            Some(偏移) => {
                let 位 = 起 + 偏移;
                if 位 > 尾界 {
                    return None;
                }
                if &堆[位..位 + 针.len()] == 针 {
                    return Some(位);
                }
                起 = 位 + 1;
            }
            None => return None,
        }
    }
    None
}
