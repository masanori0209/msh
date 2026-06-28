# msh 互換スコア

> 自動生成: `scripts/compat-score.sh` / 計測日: 2026-06-29
> バイナリ: release ビルド, `MSH_SKIP_RC=1`

## サマリー

| 指標 | 値 |
|---|---|
| 通過 | 41 / 42 |
| **通過率** | **98%** |

代表的な `.bashrc` / スクリプト構文を `msh -c` に通した結果。L5 は非目標（bash 委譲）。

## ケース別結果

| レベル | 構文 | コマンド | 結果 |
|---|---|---|---|
| L1 基本 | 変数展開 | `export X=hi; echo $X` | ✅ |
| L1 基本 | alias | `alias ll='echo aliased'; ll` | ✅ |
| L1 基本 | パイプ | `echo hello \| wc -c` | ✅ |
| L1 基本 | リダイレクト | `echo z > /tmp/msh_cs.txt; cat /tmp/msh_cs.txt` | ✅ |
| L1 基本 | glob | `ls /etc/hostname 2>/dev/null; echo ok` | ✅ |
| L2 構文 | && チェイン | `true && echo passed` | ✅ |
| L2 構文 | || フォールバック | `false \|\| echo fallback` | ✅ |
| L2 構文 | ; 連結 | `echo a; echo b` | ✅ |
| L2 構文 | コマンド置換 $() | `echo $(echo sub)` | ✅ |
| L2 構文 | バッククォート | `echo `echo bq`` | ✅ |
| L2 構文 | 終了コード $? | `false; echo $?` | ✅ |
| L2 構文 | ヒアドキュメント | `cat <<EOF⏎hd⏎EOF` | ✅ |
| L3 スクリプト | 関数 | `f() { echo fn; }; f` | ✅ |
| L3 スクリプト | if [ ] | `if [ -d /tmp ]; then echo dir; fi` | ✅ |
| L3 スクリプト | for ループ | `for i in 1 2; do echo n$i; done` | ✅ |
| L3 スクリプト | while ループ | `i=0; while [ $i -lt 1 ]; do echo w; i=1; done` | ✅ |
| L3 スクリプト | case | `case foo in foo) echo matched;; esac` | ✅ |
| L3 スクリプト | 配列 | `arr=(a b c); echo ${arr[2]}` | ✅ |
| L3 スクリプト | 配列展開 @ | `arr=(x y); echo ${arr[@]}` | ✅ |
| L3 スクリプト | 配列長 | `arr=(a b c); echo ${#arr[@]}` | ✅ |
| L4 高度 | [[ ]] ファイル | `[[ -d /tmp ]] && echo dbracket` | ✅ |
| L4 高度 | [[ ]] 文字列== | `[[ abc == abc ]] && echo streq` | ✅ |
| L4 高度 | [[ ]] 文字列!= | `[[ a != b ]] && echo strneq` | ✅ |
| L4 高度 | [[ ]] -z 空判定 | `[[ -z "" ]] && echo empty` | ✅ |
| L4 高度 | [[ ]] -n 非空 | `[[ -n x ]] && echo nonempty` | ✅ |
| L4 高度 | set -e 中断 | `set -e; false; echo SHOULD_NOT` | ✅ |
| L4 高度 | set -u 未定義 | `set -u; echo ${UNDEF_VAR_XYZ}` | ✅ |
| L4 高度 | $PIPESTATUS | `true \| false; echo ${PIPESTATUS[0]}` | ✅ |
| L4 高度 | 算術展開 | `echo $((2 + 3 * 4))` | ✅ |
| L4 高度 | 算術+変数 | `i=5; echo $((i + 1))` | ✅ |
| L5 高度 | 連想配列 | `declare -A m; m[k]=v; echo ${m[k]}` | ✅ |
| L5 高度 | 連想配列キー | `declare -A m; m[a]=1; m[b]=2; echo ${!m[@]}` | ✅ |
| L5 高度 | 連想配列数 | `declare -A m; m[a]=1; m[b]=2; echo ${#m[@]}` | ✅ |
| L5 高度 | 添字要素代入 | `arr[2]=z; echo ${arr[2]}` | ✅ |
| L5 高度 | 既定値展開 | `echo ${UNSET_XYZ:-def}` | ✅ |
| L5 高度 | 代替値展開 | `x=1; echo ${x:+set}` | ✅ |
| L5 高度 | 接尾辞除去 | `f=a.tar.gz; echo ${f%.gz}` | ✅ |
| L5 高度 | 接頭辞除去 | `p=/usr/bin; echo ${p##*/}` | ✅ |
| L5 高度 | 全置換 | `p=a:b:c; echo ${p//:/-}` | ✅ |
| L5 高度 | 大文字化 | `s=hi; echo ${s^^}` | ✅ |
| L5 高度 | 部分文字列 | `s=hello; echo ${s:1:3}` | ✅ |
| L5 非目標 | プロセス置換 | `cat <(echo proc)` | ❌ |

## 凡例

- **L1–L2**: 対話・基本構文（目標 100%）
- **L3**: スクリプト言語（目標 100%）
- **L4**: 高度な移行体験（目標 80%+）
- **L5**: 高度構文（連想配列は対応、プロセス置換等は bash 委譲）

## 既知の制約

- 連想配列の複合初期化 `declare -A m=([k]=v)`（個別代入 `m[k]=v` は対応）
- パラメータ展開: `${var:-}` / `${var:+}` / `${var:?}` / `${var#}` / `${var%}` / `${var//}` / `${var^^}` / `${var:off:len}` 対応（`${var:=}` は値を返すが永続代入は非対応）
- 算術は四則・剰余・括弧・変数のみ（`**`・三項・ビット演算は未対応）
- プロセス置換 `<( )` / `>( )`（bash 委譲）
