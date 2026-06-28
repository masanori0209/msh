# dotfiles 互換回帰

> 自動生成: `scripts/dotfiles-compat.sh` / 計測日: 2026-06-29
> 目的: 代表的 `.bashrc` パターンが **msh -c** で通るか（本番デフォルトシェル化の信頼ゲート）

## サマリー

| 指標 | 値 |
|---|---|
| 必須パターン | 7 / 7 |
| **必須通過率** | **100%** |
| 推奨+必須 通過 | 11 / 11 |
| bash 固有（許容不可） | 2 件スキップ |

CI ゲート: 必須通過率 **≥ 85%**（現目標 100%）

## ケース別

| 区分 | パターン | コマンド | 結果 |
|---|---|---|---|
| 必須 | PATH 追記 | `export PATH="$HOME/bin:$PATH"; echo ok` | ✅ |
| 必須 | 条件 source | `[ -f /etc/hosts ] && echo hostfile_ok` | ✅ |
| 必須 | 関数定義 | `f() { echo fn; }; f` | ✅ |
| 必須 | for ループ | `for d in bin sbin; do echo $d; done \| tail -1` | ✅ |
| 必須 | if ディレクトリ | `if [ -d /tmp ]; then echo tmpdir; fi` | ✅ |
| 必須 | 連想配列 | `declare -A c; c[k]=v; echo ${c[k]}` | ✅ |
| 必須 | インライン while | `i=0; while [ $i -lt 1 ]; do echo w; i=1; done` | ✅ |
| 推奨 | HISTSIZE 代入 | `HISTSIZE=5000; echo $HISTSIZE` | ✅ |
| 推奨 | プレーン代入 | `EDITOR=vim; echo $EDITOR` | ✅ |
| 推奨 | [[ 条件 | `[[ -n "x" ]] && echo nn` | ✅ |
| 推奨 | 配列 | `ports=(80 443); echo ${ports[1]}` | ✅ |
| 許容不可 | shopt | `shopt -s nullglob 2>/dev/null; echo shopt_ok` | ⏭ |
| 許容不可 | プロセス置換 | `cat <(echo x) 2>/dev/null` | ⏭ |
