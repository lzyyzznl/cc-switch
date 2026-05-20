#!/usr/bin/env bash
# [Custom] 二次开发: 上游同步辅助脚本
set -euo pipefail

UPSTREAM_REMOTE="upstream"
SYNC_TAG="last-upstream-sync"
MAIN_BRANCH="main"

usage() {
    echo "用法: $0 [--fetch|--merge|--status]"
    echo "  --fetch     获取上游更新并显示新提交"
    echo "  --merge     执行 git merge --no-ff --no-commit"
    echo "  --status    显示当前同步状态"
    exit 1
}

check_env() {
    local branch
    branch=$(git rev-parse --abbrev-ref HEAD)
    if [ "$branch" != "$MAIN_BRANCH" ]; then
        echo "错误: 必须在 $MAIN_BRANCH 分支上执行 (当前: $branch)"
        exit 1
    fi
    if ! git diff --quiet; then
        echo "错误: 工作区有未提交的改动，请先提交或 stash"
        exit 1
    fi
    if ! git remote get-url "$UPSTREAM_REMOTE" &>/dev/null; then
        echo "错误: 不存在 upstream remote，请先添加"
        echo "  git remote add upstream https://github.com/farion1231/cc-switch.git"
        exit 1
    fi
}

find_last_sync() {
    if git tag -l "$SYNC_TAG" | grep -q .; then
        local sha
        sha=$(git rev-list -1 "$SYNC_TAG")
        echo "$sha"
    else
        # 首次同步: 使用 fork 点
        git merge-base HEAD "$UPSTREAM_REMOTE/main"
    fi
}

cmd_status() {
    check_env
    echo "=== 分支: $(git rev-parse --abbrev-ref HEAD) ==="
    local last_sync
    last_sync=$(find_last_sync)
    echo "上次同步点: $(git log --oneline -1 "$last_sync" 2>/dev/null || echo '无')"
    local ahead behind
    ahead=$(git rev-list --count "$UPSTREAM_REMOTE/main"..HEAD 2>/dev/null || echo 0)
    behind=$(git rev-list --count HEAD.."$UPSTREAM_REMOTE/main" 2>/dev/null || echo 0)
    echo "落后上游: $behind 个提交 | 领先上游: $ahead 个提交"
}

cmd_fetch() {
    check_env
    echo "=== 获取上游更新 ==="
    git fetch "$UPSTREAM_REMOTE"

    local last_sync
    last_sync=$(find_last_sync)
    echo ""
    echo "上次同步点: $(git log --oneline -1 "$last_sync" 2>/dev/null || echo '无')"
    echo ""

    local new_commits
    new_commits=$(git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" 2>/dev/null)
    if [ -z "$new_commits" ]; then
        echo "✓ 上游自上次同步后无新提交"
        exit 0
    fi

    echo "=== 新提交清单 ($(echo "$new_commits" | wc -l) 个) ==="
    echo "$new_commits"

    echo ""
    echo "=== 按类型分类 ==="
    echo "fix  (故障修复):"
    local fix_commits
    fix_commits=$(git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" --grep="fix" 2>/dev/null || true)
    if [ -n "$fix_commits" ]; then
        echo "$fix_commits"
    else
        echo "  无"
    fi
    echo "feat (新特性):"
    local feat_commits
    feat_commits=$(git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" --grep="feat" 2>/dev/null || true)
    if [ -n "$feat_commits" ]; then
        echo "$feat_commits"
    else
        echo "  无"
    fi
    echo "chore/docs/其他:"
    local other_commits
    other_commits=$(git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" --grep="feat\|fix" --invert-grep 2>/dev/null || true)
    if [ -n "$other_commits" ]; then
        echo "$other_commits"
    else
        echo "  无"
    fi

    echo ""
    echo "=== 预测冲突文件 ==="
    local upstream_files
    upstream_files=$(git diff --name-only "$last_sync..$UPSTREAM_REMOTE/main" 2>/dev/null || true)
    local custom_files
    custom_files=$(git diff --name-only "$last_sync..HEAD" 2>/dev/null || true)
    if [ -n "$upstream_files" ] && [ -n "$custom_files" ]; then
        comm -12 <(echo "$upstream_files" | sort) <(echo "$custom_files" | sort) 2>/dev/null | while read -r f; do
            if [ -n "$f" ]; then
                echo "  ⚠️  $f（双方都有修改，可能冲突）"
            fi
        done
    else
        echo "  无可预测的冲突"
    fi
}

cmd_merge() {
    check_env
    echo "=== 开始合并 upstream/main ==="
    git merge "$UPSTREAM_REMOTE/main" --no-ff --no-commit || true

    if git diff --name-only --diff-filter=U | grep -q .; then
        echo ""
        echo "⚠️  存在冲突文件:"
        git diff --name-only --diff-filter=U | while read -r f; do
            echo "  - $f"
        done
        echo ""
        echo "请使用 /sync-upstream skill 逐文件处理冲突"
    else
        echo "✓ 无冲突，所有文件已暂存"
    fi
}

case "${1:-}" in
    --status) cmd_status ;;
    --fetch)  cmd_fetch ;;
    --merge)  cmd_merge ;;
    *)        usage ;;
esac
