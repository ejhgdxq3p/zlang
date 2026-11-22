#!/bin/bash
# 双引擎回归：把 测试/智语言代码项目/源码（以及 测试/ 下其余「…项目」目录的 源码/）里的
# 全部 .智 用例分别以解释模式（运行）和编译模式（构建运行）执行，逐例判定。
#
# 用法（在仓库根目录）：
#   cargo build
#   测试/双引擎回归.sh [结果目录] [并行数]
# 环境变量：
#   ZHI_BIN=路径       换一个可执行文件来跑（做「改动前后对照」时指向基线版本的构建产物）
#   STRICT_DEPS=1      缺 C 编译器/openssl/python 或有用例自行跳过时判失败（持续集成用）
#
# 判定口径见 文档/智语言解释器编译器技术方案.md 5.6 节，实现在本文件的 judge()：
#   · 负向用例（文件头有 // @预期错误）必须真的出错；有 // @期望包含 时输出必须含该片段，
#     没有时拒绝 panic 等与被测条款无关的错误。
#   · 正向用例必须退出码为 0、没有 ❌ / FAILCASE、「通过：N / M」里 N = M。
#   · 超时单列为失败。没有 启动程序 也没有注解的文件是被别的用例导入的模块，不当用例跑。
#   · 同目录有 <用例名>.输入.txt 时作为标准输入喂给用例。
# 退出码：0 = 两种模式下每个用例都通过（测试/已知失败.txt 里登记的除外）；1 = 有失败；2 = 没法跑。
# 两种模式输出文本不同的用例只列出、不计入失败（异常堆栈格式、随机值、时间戳等已知差异）。
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${ZHI_BIN:-"$ROOT/产出/debug/智语言开发套件"}
OUT=${1:-"$ROOT/产出/双引擎回归"}
JOBS=${2:-6}
[ -x "$BIN" ] || { echo "未找到 $BIN，请先 cargo build --bin 智语言开发套件"; exit 2; }
mkdir -p "$OUT/解释" "$OUT/编译"
OUT=$(cd "$OUT" && pwd)   # 转成绝对路径：下面会切换目录，相对路径会指到别处
cd "$ROOT/测试/智语言代码项目" || exit 2

# ── 准备用例依赖 ────────────────────────────────────────────────────────────
MISSING=""
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
    || { echo "提示：外部语言样例库编译失败，外部语言用例会失败"; MISSING=1; }
else
  echo "提示：没有 C 编译器（clang/cc/gcc），外部语言用例会失败"; MISSING=1
fi
if [ ! -s "$FIX/cert.pem" ] || [ ! -s "$FIX/key.pem" ]; then
  if command -v openssl > /dev/null 2>&1; then
    # 证书主题和扩展写进配置文件而不是命令行：Windows 的 Git Bash 会把 /CN=… 这样的参数当路径改写
    printf '%s\n' '[req]' 'distinguished_name = dn' 'x509_extensions = ext' 'prompt = no' \
      '[dn]' 'CN = localhost' '[ext]' 'subjectAltName = IP:127.0.0.1,DNS:localhost' > "$FIX/openssl.cnf"
    (cd "$FIX" && openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config openssl.cnf \
      -keyout key.pem -out cert.pem > openssl.log 2>&1) \
      || { rm -f "$FIX/cert.pem" "$FIX/key.pem"; echo "提示：测试证书生成失败，TLS 用例会跳过或失败："; tail -5 "$FIX/openssl.log"; MISSING=1; }
  else
    echo "提示：没有 openssl，TLS 用例会跳过或失败"; MISSING=1
  fi
fi
printf 'ab\xff\xfecd\n' > "$FIX/坏字节.bin"
HELPER_PID=""
if PY=$(find_tool python3 python); then
  "$PY" "$ROOT/测试/辅助/坏字节对端.py" & HELPER_PID=$!
  trap '[ -n "$HELPER_PID" ] && kill "$HELPER_PID" 2> /dev/null' EXIT
  # 等对端开始监听，免得头几个用例连不上
  for _ in $(seq 50); do (exec 3<> /dev/tcp/127.0.0.1/18391) 2> /dev/null && break; sleep 0.1; done
else
  echo "提示：没有 python，非法字节用例会跳过"; MISSING=1
fi

# 判定一个用例的一次运行：输出「通过|失败|跳过<TAB>原因」。
judge() {
  local SRC=$1 LOG=$2 CODE=$3 FRAG
  case $CODE in 124|137) printf '失败\t超时\n'; return ;; esac
  if grep -q '^//[[:space:]]*@预期错误' "$SRC"; then
    [ "$CODE" != 0 ] || { printf '失败\t预期的错误没有出现\n'; return; }
    # 片段可以和注解同行，也可以注解独占一行、片段写在下一行注释里
    FRAG=$(awk '/^\/\/[[:space:]]*@期望包含/ { sub(/^\/\/[[:space:]]*@期望包含[[:space:]]*/, ""); if ($0 != "") print; else want = 1; next }
                want { sub(/^\/\/[[:space:]]*/, ""); print; want = 0 }' "$SRC" | tr -d '\r')
    if [ -n "$FRAG" ]; then
      while IFS= read -r F; do
        grep -aqF -- "$F" "$LOG" || { printf '失败\t输出里没有期望的片段：%s\n' "$F"; return; }
      done <<< "$FRAG"
    elif grep -aq 'panicked at\|程序入口重复定义\|与内建标准库冲突\|未找到测试函数或启动程序' "$LOG"; then
      printf '失败\t报的是与被测条款无关的错误\n'; return
    fi
    printf '通过\t\n'; return
  fi
  [ "$CODE" = 0 ] || { printf '失败\t退出码 %s\n' "$CODE"; return; }
  grep -aq '❌\|FAILCASE' "$LOG" && { printf '失败\t有失败的检查点\n'; return; }
  grep -a '通过[：:] *[0-9][0-9]* */ *[0-9]' "$LOG" | sed 's/.*通过[：:] *\([0-9]*\) *\/ *\([0-9]*\).*/\1 \2/' \
    | awk '$1 != $2 { bad = 1 } END { exit !bad }' && { printf '失败\t「通过：N / M」里 N 小于 M\n'; return; }
  grep -aq '^跳过：' "$LOG" && { printf '跳过\t%s\n' "$(grep -a '^跳过：' "$LOG" | head -1)"; return; }
  printf '通过\t\n'
}

run_one() {
  local MODE=$1 DIR=$2 FILE=$3 NAME INPUT CODE
  NAME=$(basename "$FILE" .智); INPUT="${FILE%.智}.输入.txt"
  grep -q '^启动程序\|^//[[:space:]]*@预期错误' "$FILE" || return 0
  [ -f "$INPUT" ] || INPUT=/dev/null
  timeout -k 5 120 "$BIN" "$MODE" "$FILE" > "$DIR/$NAME.log" 2>&1 < "$INPUT"; CODE=$?
  echo "EXIT=$CODE" >> "$DIR/$NAME.log"
  printf '%s\t%s\n' "$NAME" "$(judge "$FILE" "$DIR/$NAME.log" "$CODE")" > "$DIR/$NAME.判定"
}
export -f run_one judge; export BIN

run_all() {
  local MODE=$1 DIR=$2
  rm -f "$DIR"/*.log "$DIR"/*.判定
  # 含「外部:」块的用例也一起并行跑：FFI 包装按内容摘要命名，多个进程互不覆盖。
  printf '%s\n' 源码/*.智 | xargs -P "$JOBS" -I{} bash -c 'run_one "$0" "$1" "$2"' "$MODE" "$DIR" {}
  # 其余自带配置的小项目（依赖声明写在它自己的 项目.配置 里）。用例名不能与主项目重复。
  for PROJ in "$ROOT"/测试/*项目; do
    [ "$PROJ" = "$ROOT/测试/智语言代码项目" ] && continue
    (cd "$PROJ" && for FILE in 源码/*.智; do [ -f "$FILE" ] && run_one "$MODE" "$DIR" "$FILE"; done)
  done
  cat "$DIR"/*.判定 | sort > "$DIR/汇总.tsv"
}

echo "[1/2] 解释模式…"; run_all 运行 "$OUT/解释"
echo "[2/2] 编译模式…"; run_all 构建运行 "$OUT/编译"
[ -z "$HELPER_PID" ] || kill -0 "$HELPER_PID" 2> /dev/null || { echo "坏字节对端中途退出了，相关用例的结果不可信"; MISSING=1; }

flt() { grep -av '^\[智语言\]\|字节码：\|编译完成' "$1"; }
: > "$OUT/输出差异.txt"
for LOG in "$OUT/解释"/*.log; do
  NAME=$(basename "$LOG")
  diff -q <(flt "$LOG") <(flt "$OUT/编译/$NAME") > /dev/null 2>&1 || echo "${NAME%.log}" >> "$OUT/输出差异.txt"
done

TOTAL=$(wc -l < "$OUT/解释/汇总.tsv")
echo "用例总数：$TOTAL"
[ "${TOTAL:-0}" -gt 0 ] || { echo "没有跑到任何用例，回归无效"; exit 2; }
# 每行：模式 用例名 结论 原因
awk -F'\t' -v OFS='\t' '{ print MODE, $0 }' MODE=解释 "$OUT/解释/汇总.tsv" MODE=编译 "$OUT/编译/汇总.tsv" > "$OUT/逐例结论.tsv"
FAILED=$(awk -F'\t' '$3 == "失败" { print $2 }' "$OUT/逐例结论.tsv" | sort -u)
SKIPPED=$(awk -F'\t' '$3 == "跳过" { print $2 }' "$OUT/逐例结论.tsv" | sort -u)
KNOWN=$(grep -v '^#' "$ROOT/测试/已知失败.txt" 2> /dev/null | tr -d '\r' | grep .)
NEWFAILED=$(printf '%s\n' "$FAILED" | grep -vxF -f <(printf '%s\n' "$KNOWN") | grep .)
FIXED=$(printf '%s\n' "$KNOWN" | grep -vxF -f <(printf '%s\n' "$FAILED") | grep .)

echo "失败的用例：$(printf '%s' "$FAILED" | grep -c .)（其中不在已知失败清单里的：$(printf '%s' "$NEWFAILED" | grep -c .)）"
awk -F'\t' '$3 == "失败" { printf "  [%s] %s：%s\n", $1, $2, $4 }' "$OUT/逐例结论.tsv" | sort -k2
[ -z "$FIXED" ] || { echo "已知失败清单里这些现在通过了，请从清单删掉："; printf '%s\n' "$FIXED" | sed 's/^/  /'; }
[ -z "$SKIPPED" ] || { echo "自行跳过的用例："; printf '%s\n' "$SKIPPED" | sed 's/^/  /'; }
echo "输出文本双模不同（仅提示）：$(wc -l < "$OUT/输出差异.txt")，清单见 $OUT/输出差异.txt"
if [ -n "${STRICT_DEPS:-}" ] && { [ -n "$MISSING" ] || [ -n "$SKIPPED" ]; }; then
  echo "STRICT_DEPS：缺少依赖或有用例跳过，判失败"; exit 1
fi
[ -z "$NEWFAILED" ]
