#!/usr/bin/env bash
# PR 的 CI 入口闸门（ci.md §2「PR 运行的并发上限」）：同时「过了闸、在跑重 job」的 PR 运行不超过 $CAP 个，
# 其余在这里排队，先来先过。不限的话，几个 PR 一起推就会把账号的 runner 池（约 20 个并发 job）挤满，
# 每个 PR 都慢；main push / schedule / dispatch 不经过这里。
#
# 「过了闸」= 该运行里已经出现重 job（闸门在 detect-changes 里，它一结束下游 job 才会被创建）。
# 排队顺序按 run_number：只看比自己早的、仍未结束的 PR 运行——
#   放行条件：比我早且已过闸的运行数 + 比我早且还在排队的运行数 < CAP。
# 两个运行同时判定可能各自放行、短暂超出 1 个，可以接受。等满 $MAX_WAIT_MIN 分钟一律放行（防卡死）——
# 这个阀要远大于一次 PR 运行在拥挤时的时长（实测 1 小时以上），否则排队的运行会被它批量放行，上限形同虚设。
# 只用到比自己早的运行，API 调用数随排队位置增长而不是随 PR 总数增长。
#
# 环境：GH_TOKEN、GITHUB_REPOSITORY、GITHUB_RUN_ID、GITHUB_RUN_NUMBER；CAP（默认 2）、MAX_WAIT_MIN（默认 240）、
# POLL_S（默认 60）。
set -euo pipefail

repo="${GITHUB_REPOSITORY:-z42-lang/z42}"
cap="${CAP:-2}"
max_wait_min="${MAX_WAIT_MIN:-240}"
poll_s="${POLL_S:-60}"
me="${GITHUB_RUN_NUMBER:?}"
start=$(date +%s)

# 一个运行的状态：running（已过闸、有重 job）| waiting（detect-changes 还没结束，即还在闸门里）| light（只有
# detect-changes / docs-check 这类轻量 job，不占名额，如纯文档 PR）。
run_state() {
  gh api "repos/$repo/actions/runs/$1/jobs?per_page=100" -q '
    [.jobs[] | select(.name != "detect-changes" and (.name | startswith("docs-check") | not))] as $heavy
    | [.jobs[] | select(.name == "detect-changes" and .status != "completed")] as $gate
    | if ($heavy | length) > 0 then "running" elif ($gate | length) > 0 then "waiting" else "light" end' \
    2>/dev/null || echo waiting
}

while true; do
  ahead_running=0
  ahead_waiting=0
  # 带过滤的运行列表是最终一致的，只用来找候选（同 main-supersede.sh）。
  while IFS=$'\t' read -r id num; do
    [ -z "$id" ] && continue
    [ "$id" = "${GITHUB_RUN_ID:-}" ] && continue
    [ "$num" -ge "$me" ] && continue
    case "$(run_state "$id")" in
      running) ahead_running=$((ahead_running + 1)) ;;
      waiting) ahead_waiting=$((ahead_waiting + 1)) ;;
    esac
  done < <(gh api "repos/$repo/actions/workflows/ci.yml/runs?event=pull_request&per_page=100" \
             -q '.workflow_runs[] | select(.status != "completed") | [.id, .run_number] | @tsv' 2>/dev/null || true)

  waited=$(( ($(date +%s) - start) / 60 ))
  if [ $((ahead_running + ahead_waiting)) -lt "$cap" ]; then
    echo "放行：前面在跑 $ahead_running、在排队 $ahead_waiting（上限 $cap），等了 ${waited} 分钟"
    exit 0
  fi
  if [ "$waited" -ge "$max_wait_min" ]; then
    echo "::warning::等满 ${max_wait_min} 分钟仍未轮到（前面在跑 $ahead_running、在排队 $ahead_waiting），放行"
    exit 0
  fi
  echo "排队：前面在跑 $ahead_running、在排队 $ahead_waiting（上限 $cap），已等 ${waited} 分钟"
  sleep "$poll_s"
done
