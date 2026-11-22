#!/bin/bash
# 双引擎回归：把 测试/智语言代码项目/源码 下的全部 .智 用例分别以解释模式（运行）和
# 编译模式（构建运行）执行，逐个比较「退出码」与「❌ 行数」，并列出输出文本不同的用例。
#
# 用法（在仓库根目录）：
#   cargo build --bin 智语言开发套件
#   测试/双引擎回归.sh [结果目录] [并行数]
#
# 退出码：0 = 两种模式逐例一致且没有新的 ❌；1 = 存在不一致或失败用例。
#   测试/已知失败.txt 里列出的用例（每行一个用例名，# 开头为注释）照常运行和列出，
#   但它们含 ❌ 不影响退出码——用于登记「接手前就失败、尚未修」的用例，免得持续集成永远是红的。
# 说明：
#   · 负向用例（文件名含「负向」等）本来就以非零退出，属正常；这里只要求两种模式一致。
#   · 输出文本差异目前仍有若干已知项（异常位置/堆栈格式、标准错误先后顺序、
#     随机值与时间戳），只列出、不计入失败。
#   · 运行前会准备几样用例依赖的东西（缺工具时跳过并提示，相关用例自行跳过或按原样失败）：
#       C 编译器（clang/cc/gcc） → 编译 测试/外部语言样例/*.c 为外部语言用例要用的动态库
#       openssl                  → 生成自签测试证书到 测试/夹具产物/（私钥不入库）
#       python                   → 后台启动 测试/辅助/坏字节对端.py（非法字节用例的对端）
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="$ROOT/产出/debug/智语言开发套件"
OUT=${1:-"$ROOT/产出/双引擎回归"}
JOBS=${2:-6}
[ -x "$BIN" ] || { echo "未找到 $BIN，请先 cargo build --bin 智语言开发套件"; exit 2; }
mkdir -p "$OUT/解释" "$OUT/编译"
OUT=$(cd "$OUT" && pwd)   # 转成绝对路径：下面会切换目录，相对路径会指到别处
cd "$ROOT/测试/智语言代码项目" || exit 2

# ── 准备用例依赖 ────────────────────────────────────────────────────────────
find_tool() { for T in "$@"; do command -v "$T" > /dev/null 2>&1 && { echo "$T"; return 0; }; done; return 1; }
FIX="$ROOT/测试/夹具产物"; SAMPLE="$ROOT/测试/外部语言样例"
mkdir -p "$FIX" 外部语言
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) DLL_PREFIX=""; DLL_EXT="dll"; PIC="" ;;
  Darwin)               DLL_PREFIX="lib"; DLL_EXT="dylib"; PIC="-fPIC" ;;
  *)                    DLL_PREFIX="lib"; DLL_EXT="so"; PIC="-fPIC" ;;
esac
if CC=$(find_tool clang cc gcc); then
  # 真实 C 接口的库放 外部语言/（shim 的搜索目录）；扁平契约的库放项目根（按当前目录加载）
  "$CC" -shared $PIC -O1 -o "外部语言/${DLL_PREFIX}嵌套结构库.$DLL_EXT" "$SAMPLE/嵌套结构库.c" \
    && "$CC" -shared $PIC -O1 -o "样例库.$DLL_EXT" "$SAMPLE/样例库.c" \
    || echo "提示：外部语言样例库编译失败，外部语言用例会失败"
else
  echo "提示：没有 C 编译器（clang/cc/gcc），外部语言用例会失败"
fi
if [ ! -s "$FIX/cert.pem" ] || [ ! -s "$FIX/key.pem" ]; then
  if command -v openssl > /dev/null 2>&1; then
    # 证书主题和扩展写进配置文件而不是命令行：Windows 的 Git Bash 会把 /CN=… 这样的参数当路径改写
    printf '%s\n' '[req]' 'distinguished_name = dn' 'x509_extensions = ext' 'prompt = no' \
      '[dn]' 'CN = localhost' '[ext]' 'subjectAltName = IP:127.0.0.1,DNS:localhost' > "$FIX/openssl.cnf"
    (cd "$FIX" && openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config openssl.cnf \
      -keyout key.pem -out cert.pem > openssl.log 2>&1) \
      || { rm -f "$FIX/cert.pem" "$FIX/key.pem"; echo "提示：测试证书生成失败，TLS 用例会跳过或失败："; tail -5 "$FIX/openssl.log"; }
  else
    echo "提示：没有 openssl，TLS 用例会跳过或失败"
  fi
fi
printf 'ab\xff\xfecd\n' > "$FIX/坏字节.bin"
HELPER_PID=""
if PY=$(find_tool python3 python); then
  "$PY" "$ROOT/测试/辅助/坏字节对端.py" & HELPER_PID=$!
  trap '[ -n "$HELPER_PID" ] && kill "$HELPER_PID" 2> /dev/null' EXIT
else
  echo "提示：没有 python，非法字节用例会跳过"
fi

run_one() {
  MODE=$1; DIR=$2; FILE=$3; NAME=$(basename "$FILE" .智)
  timeout 120 "$BIN" "$MODE" "$FILE" > "$DIR/$NAME.log" 2>&1 < /dev/null
  echo "EXIT=$?" >> "$DIR/$NAME.log"
}
export -f run_one; export BIN

run_all() {
  MODE=$1; DIR=$2
  rm -f "$DIR"/*.log
  # 含「外部:」块的用例逐个串行跑：FFI 包装（shim）生成在固定目录、固定文件名下
  # （解释模式是全局目录，编译模式是项目产出目录），多个进程同时生成会互相覆盖，
  # 表现为段错误或中止。这是工具链现存的限制，不是用例本身的问题。
  grep -L '^外部:' 源码/*.智 | xargs -P "$JOBS" -I{} bash -c 'run_one "$0" "$1" "$2"' "$MODE" "$DIR" {}
  grep -l '^外部:' 源码/*.智 | xargs -P 1 -I{} bash -c 'run_one "$0" "$1" "$2"' "$MODE" "$DIR" {}
  for LOG in "$DIR"/*.log; do
    CODE=$(grep -a '^EXIT=' "$LOG" | tail -1 | cut -d= -f2)
    FAILS=$(grep -a -c '❌' "$LOG")
    printf '%s\t%s\t%s\n' "$(basename "$LOG" .log)" "$CODE" "$FAILS"
  done | sort > "$DIR/汇总.tsv"
}

echo "[1/2] 解释模式…"; run_all 运行 "$OUT/解释"
echo "[2/2] 编译模式…"; run_all 构建运行 "$OUT/编译"

flt() { grep -av '^\[智语言\]\|字节码：\|编译完成' "$1"; }
TOTAL=$(wc -l < "$OUT/解释/汇总.tsv")
MISMATCH=$(join -t$'\t' "$OUT/解释/汇总.tsv" "$OUT/编译/汇总.tsv" | awk -F'\t' '$2!=$4 || $3!=$5')
FAILED=$(awk -F'\t' '$3>0 {print $1}' "$OUT/解释/汇总.tsv" "$OUT/编译/汇总.tsv" | sort -u)
: > "$OUT/输出差异.txt"
for LOG in "$OUT/解释"/*.log; do
  NAME=$(basename "$LOG")
  diff -q <(flt "$LOG") <(flt "$OUT/编译/$NAME") > /dev/null 2>&1 || echo "${NAME%.log}" >> "$OUT/输出差异.txt"
done

echo "用例总数：$TOTAL"
[ "${TOTAL:-0}" -gt 0 ] || { echo "没有跑到任何用例，回归无效"; exit 2; }
echo "退出码或失败数双模不一致：$(printf '%s' "$MISMATCH" | grep -c .)"
[ -n "$MISMATCH" ] && printf '%s\n' "$MISMATCH" | sed 's/^/  /'
KNOWN="$ROOT/测试/已知失败.txt"
NEWFAILED=$FAILED
[ -f "$KNOWN" ] && NEWFAILED=$(printf '%s\n' "$FAILED" | grep -vxF -f <(grep -v '^#' "$KNOWN" | tr -d '\r' | grep .) | grep .)
echo "含 ❌ 的用例：$(printf '%s' "$FAILED" | grep -c .)（其中不在已知失败清单里的：$(printf '%s' "$NEWFAILED" | grep -c .)）"
[ -n "$FAILED" ] && printf '%s\n' "$FAILED" | sed 's/^/  /'
echo "输出文本双模不同（仅提示）：$(wc -l < "$OUT/输出差异.txt")，清单见 $OUT/输出差异.txt"
[ -z "$MISMATCH" ] && [ -z "$NEWFAILED" ]
