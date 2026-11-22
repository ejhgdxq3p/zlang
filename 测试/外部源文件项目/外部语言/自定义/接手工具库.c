/* 回归用例 311 的外部依赖：以源文件形式声明，由工具链现场编译。
   故意不包含任何头文件，这样只要有 C 编译器就能编。 */
static long long counter = 0;

long long src_add(long long a, long long b) { return a + b + 1000; }
double src_half(double v) { return v / 2.0; }
long long src_next(void) { counter += 1; return counter; }
