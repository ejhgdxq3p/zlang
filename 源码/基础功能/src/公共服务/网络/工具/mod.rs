pub use self::自定义流::CustomStream;
pub use self::等值读取器::EqualReader;
pub use self::融合读取器::FusedReader;
pub use self::消息队列::MessagesQueue;
pub use self::封装流::RefinedTcpStream;
pub use self::顺序读取器::{SequentialReader, SequentialReaderBuilder};
pub use self::顺序读取器::{SequentialWriter, SequentialWriterBuilder};
pub use self::任务池::TaskPool;

use std::str::FromStr;

#[path = "自定义流.rs"] mod 自定义流;
#[path = "等值读取器.rs"] mod 等值读取器;
#[path = "融合读取器.rs"] mod 融合读取器;
#[path = "消息队列.rs"] mod 消息队列;
#[path = "封装流.rs"] pub(crate) mod 封装流;
#[path = "顺序读取器.rs"] mod 顺序读取器;
#[path = "任务池.rs"] mod 任务池;

/// Parses a the value of a header.
/// Suitable for `Accept-*`, `TE`, etc.
///
/// For example with `text/plain, image/png; q=1.5` this function would
/// return `[ ("text/plain", 1.0), ("image/png", 1.5) ]`
pub fn parse_header_value(input: &str) -> Vec<(&str, f32)> {
    input
        .split(',')
        .filter_map(|elem| {
            let mut params = elem.split(';');

            let t = params.next()?;

            let mut value = 1.0_f32;

            for p in params {
                if p.trim_start().starts_with("q=") {
                    if let Ok(val) = f32::from_str(p.trim_start()[2..].trim()) {
                        value = val;
                        break;
                    }
                }
            }

            Some((t.trim(), value))
        })
        .collect()
}

#[cfg(test)]
mod test {
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_parse_header() {
        let result = super::parse_header_value("text/html, text/plain; q=1.5 , image/png ; q=2.0");

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, "text/html");
        assert_eq!(result[0].1, 1.0);
        assert_eq!(result[1].0, "text/plain");
        assert_eq!(result[1].1, 1.5);
        assert_eq!(result[2].0, "image/png");
        assert_eq!(result[2].1, 2.0);
    }
}
