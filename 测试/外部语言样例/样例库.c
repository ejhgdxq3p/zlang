/* 回归用例「118外部语言调用路由与边界」依赖的库（智语言扁平契约）。
 * 扁平契约下每个导出函数的签名固定为 void f(输出, 实参个数, 实参数组)，
 * 值的内存布局与运行时的「运行时值」一致（32 字节）。 */

#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT
#endif

typedef struct {
    unsigned long long tag;   /* 0 空值，1 整数，2 小数 … */
    long long integer;
    double decimal;
    unsigned long long handle;
} Value;

EXPORT void add(Value *out, long long count, Value *args) {
    out->tag = 1;
    out->integer = count >= 2 ? args[0].integer + args[1].integer : 0;
    out->decimal = 0.0;
    out->handle = 0;
}
