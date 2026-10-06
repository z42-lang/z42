#!/usr/bin/env bash
# main 上 push 触发的 CI 运行之间的「替换」规则（ci.md §2「main 上的运行不互相打断」）。
#
# main 的 push 运行各占一个 concurrency group（GitHub 不再自动取消），由每个新运行在 detect-changes
# 里调本脚本决定：比自己早、还没结束的 main push 运行里，**不影响 SDK** 的取消，**影响 SDK** 的留着跑完
# 并发布 nightly —— 后面「support 先行、晚一个 nightly 再 use」的提交要靠那份 nightly 当种子。
#
# 「影响 SDK」按 **当前 nightly → 该运行的 commit** 的累计改动判（不是只看那一个 commit）：
# nightly 还没收进的 SDK 改动，后面任何运行都带着，它们也得发布。判不清（拿不到 nightly、
# compare 截断）一律当影响。
#
# 用法：
#   main-supersede.sh sdk <base-sha> <head-sha>   打印 true / false
#   main-supersede.sh supersede                    取消较早的、不影响 SDK 的 main push 运行；
#                                                  打印本运行自己的 sdk=true|false（写 $GITHUB_OUTPUT 若有）
# 环境：GH_TOKEN、GITHUB_REPOSITORY、GITHUB_SHA、GITHUB_RUN_ID、GITHUB_RUN_NUMBER；DRY_RUN=1 只打印不取消。
set -euo pipefail

repo="${GITHUB_REPOSITORY:-z42-lang/z42}"

# 不进 SDK 的路径（逐个文件判；**全部**命中才算不影响）：文档、测试源码与夹具、bench、示例、
# xtask 里只做测试编排的部分、不参与发布的 workflow。其余一律算影响 —— 包括 xtask 的
# build / package / common / install（它们决定 SDK 里装什么）、ci.yml、.github/actions。
non_sdk_re='^(docs/|examples/|\.claude/)|\.md$|(^|/)(tests|bench|benches)/|^scripts/test/|^scripts/cli/xtask_cli_(test|check)\.z42$|^scripts/xtask_(bench|profile)\.z42$|^\.github/workflows/(bench-pr|deploy-book|jit-fixpoint-check)\.yml$'

# 一组文件里有没有影响 SDK 的（stdin 每行一个路径）。
touches_sdk() {
  local hit
  hit=$(grep -v '^$' | grep -Ev "$non_sdk_re" | head -1 || true)
  [ -n "$hit" ]
}

# base..head 是否影响 SDK：true / false。
sdk_between() {
  local base="$1" head="$2" json n
  if [ -z "$base" ]; then echo true; return; fi
  if [ "$base" = "$head" ]; then echo false; return; fi
  if ! json=$(gh api "repos/$repo/compare/$base...$head" 2>/dev/null); then echo true; return; fi
  # compare 最多返回 300 个文件；到顶就当截断 ⇒ 保守。
  n=$(printf '%s' "$json" | jq '.files | length')
  if [ "$n" -ge 300 ]; then echo true; return; fi
  if printf '%s' "$json" | jq -r '.files[].filename' | touches_sdk; then echo true; else echo false; fi
}

nightly_sha() {
  gh api "repos/$repo/git/ref/tags/nightly" -q .object.sha 2>/dev/null || true
}

case "${1:-}" in
  sdk)
    sdk_between "$2" "$3"
    ;;
  supersede)
    base=$(nightly_sha)
    echo "nightly @ ${base:-<none>}"
    mine=$(sdk_between "$base" "$GITHUB_SHA")
    echo "this run ($GITHUB_SHA): sdk=$mine"
    if [ -n "${GITHUB_OUTPUT:-}" ]; then echo "sdk=$mine" >> "$GITHUB_OUTPUT"; fi
    # 带过滤的运行列表是最终一致的（ci.md「回退」一节）：只拿来找候选，取消前逐个再查一次状态。
    gh api "repos/$repo/actions/workflows/ci.yml/runs?branch=main&event=push&per_page=50" \
      -q '.workflow_runs[] | select(.status != "completed") | [.id, .run_number, .head_sha] | @tsv' |
    while IFS=$'\t' read -r id num sha; do
      [ "$id" = "${GITHUB_RUN_ID:-}" ] && continue
      [ "$num" -ge "${GITHUB_RUN_NUMBER:-0}" ] && continue          # 只处理比自己早的
      status=$(gh api "repos/$repo/actions/runs/$id" -q .status)
      [ "$status" = "completed" ] && continue
      if [ "$(sdk_between "$base" "$sha")" = "true" ]; then
        echo "keep   #$num ($sha): 带着 nightly 尚未收进的 SDK 改动，留它跑完发布"
      elif [ "${DRY_RUN:-}" = "1" ]; then
        echo "cancel #$num ($sha): 不影响 SDK（dry run）"
      else
        echo "cancel #$num ($sha): 不影响 SDK，被本运行替换"
        gh api -X POST "repos/$repo/actions/runs/$id/cancel" >/dev/null || echo "  (cancel failed — 可能刚结束)"
      fi
    done
    ;;
  *)
    echo "usage: $0 sdk <base> <head> | supersede" >&2
    exit 2
    ;;
esac
