// 文件: 核心库功能/数据格式系统函数.rs
// 职责: JSON 与 运行时值 双向转换辅助

use crate::值类型::*;
use serde_json;

pub(crate) fn json值转运行时值(v: &serde_json::Value) -> 运行时值 {
    match v {
        serde_json::Value::Null => 运行时值::空值(),
        serde_json::Value::Bool(b) => 运行时值::逻辑(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { 运行时值::整数(i) }
            else { 运行时值::小数(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => 分配文本(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<运行时值> = arr.iter().map(json值转运行时值).collect();
            分配数组(items)
        }
        serde_json::Value::Object(obj) => {
            let items: Vec<(堆键, 运行时值)> = obj.iter()
                .map(|(k, v)| (堆键::文本(k.clone()), json值转运行时值(v)))
                .collect();
            分配映射(items)
        }
    }
}

pub(crate) fn 运行时值转json值(v: &运行时值) -> serde_json::Value {
    match v.标签 {
        标签_空值 => serde_json::Value::Null,
        标签_整数 => serde_json::Value::Number(serde_json::Number::from(v.整数)),
        标签_小数 => {
            if let Some(n) = serde_json::Number::from_f64(v.小数) {
                serde_json::Value::Number(n)
            } else {
                serde_json::Value::Null
            }
        }
        标签_逻辑 => serde_json::Value::Bool(v.整数 != 0),
        标签_句柄 => {
            match 堆读取(v.句柄) {
                Some(堆数据::文本(s)) => serde_json::Value::String(s),
                Some(堆数据::文本_静态(p, n)) => {
                    let 切片 = unsafe { std::slice::from_raw_parts(p as *const u8, n) };
                    serde_json::Value::String(String::from_utf8_lossy(切片).into_owned())
                }
                Some(堆数据::数组(arr)) => {
                    let items: Vec<serde_json::Value> = arr.iter().map(运行时值转json值).collect();
                    serde_json::Value::Array(items)
                }
                Some(堆数据::映射(m)) => {
                    let mut map = serde_json::Map::new();
                    for (k, val) in &m {
                        let key_str = match k {
                            堆键::文本(s) => s.clone(),
                            堆键::整数(i) => i.to_string(),
                            堆键::逻辑(b) => b.to_string(),
                            // 2026-10-12：新增键变体（与语言渲染一致）
                            堆键::空值 => "空值".to_string(),
                            堆键::小数(位) => 运行时值::格式化小数显示(f64::from_bits(*位)),
                            堆键::结构体 { .. } => 堆键转文本(k),
                            堆键::枚举 { .. } => 堆键转文本(k),
                        };
                        map.insert(key_str, 运行时值转json值(val));
                    }
                    serde_json::Value::Object(map)
                }
                _ => serde_json::Value::Null,
            }
        }
        _ => serde_json::Value::Null,
    }
}
