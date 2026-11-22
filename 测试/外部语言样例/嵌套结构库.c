/* 回归用例「外部语言_嵌套结构体字段」「外部语言_数组字段」依赖的 C 库（真实 C ABI）。
 * 由 测试/双引擎回归.sh 现场编译为动态库，放到 测试/智语言代码项目/外部语言/ 下。 */

#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT
#endif

#include <stddef.h>

typedef struct { long long x; long long y; } Point;
typedef struct { Point start; Point end; long long count; } Line;

EXPORT Line make_line(long long x1, long long y1, long long x2, long long y2, long long count) {
    Line line;
    line.start.x = x1; line.start.y = y1;
    line.end.x = x2;   line.end.y = y2;
    line.count = count;
    return line;
}

typedef struct { long long *data; size_t count; } DataBag;

EXPORT DataBag make_bag(void) {
    static long long values[5] = {10, 20, 30, 40, 50};
    DataBag bag;
    bag.data = values;
    bag.count = 5;
    return bag;
}

/* —— 以下供「308接手回归_外部语言原生接口」使用 —— */

EXPORT long long add_ll(long long a, long long b) { return a + b; }

EXPORT double scale(double value, double factor) { return value * factor; }

EXPORT long long text_bytes(const char *text) {
    long long n = 0;
    while (text && text[n]) n++;
    return n;
}

/* 带输出参数的 C 函数按「返回状态码 + 指针写回结果」的惯例：返回 0 表示成功，
 * 非 0 时智语言一侧抛异常。这里「商」夹在两个输入参数中间，「余数」在末尾。 */
EXPORT long long div_rem(long long a, long long *quot, long long b, long long *rem) {
    if (b == 0) return 7;
    *quot = a / b;
    *rem = a % b;
    return 0;
}

/* 只有一个输出参数、且排在最前面。 */
EXPORT long long square_out(long long *out, long long value) {
    *out = value * value;
    return 0;
}
