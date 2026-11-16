// 文件: 版本管理器/src/命令/输出格式.rs
// 职责: 统一输出格式（颜色/简洁/结构化）。被 检查/测试/构建 等命令复用，
//       保证解释模式与编译模式享有完全一致的命令行输出体验。

/// 结构化输出：将一组键值打印为单行 JSON（输出到 stdout，便于脚本解析）
pub fn 打印结构化(对象: &[(String, String)]) {
    let mut s = String::from("{");
    for (i, (k, v)) in 对象.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // 转义 JSON 特殊字符
        let 转义值 = v
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        s.push_str(&format!("\"{}\":\"{}\"", k, 转义值));
    }
    s.push('}');
    println!("{}", s);
}
