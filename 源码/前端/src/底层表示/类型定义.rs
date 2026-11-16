#[derive(Debug, Clone, PartialEq, Eq)]
pub enum 底层表示类型 {
    整数64,
    小数64,
    文本,
    逻辑,
    空值,
    指针,
    函数指针,
    结构体(usize),    // 类型索引
    数组(usize),       // 元素类型索引
    映射(usize, usize), // 键类型索引, 值类型索引
    枚举(usize),        // 类型索引
    接口型(usize),      // 类型索引
}