# Sync-Upstream & Git-Push Skills 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 创建 CLAUDE.md 规范 + `scripts/sync-upstream.sh` 辅助脚本 + 两个 Claude Code Skill（`/sync-upstream` 和 `/git-push`），实现上游代码安全合并与合规推送。

**Architecture:** 四个独立文件，层层依赖：CLAUDE.md（规范基础）→ sync-upstream.sh（机械操作）→ sync-upstream.md（同步技能）→ git-push.md（推送技能）。两个 skill 共享 CLAUDE.md 中定义的 `[Custom]`/`[Upstream]`/`[Merge]` 标注体系。

**Tech Stack:** Claude Code Skill (Markdown)、Git、Bash

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `CLAUDE.md` | 定义 fork 开发规范、代码标注规则、技能调用说明 |
| `scripts/sync-upstream.sh` | 执行机械性 git 操作：fetch、merge、tag 管理 |
| `.claude/skills/sync-upstream.md` | `/sync-upstream` 技能：引导完成上游同步全流程 |
| `.claude/skills/git-push.md` | `/git-push` 技能：注释合规检查 + 推送 |

---

### Task 1: 创建 CLAUDE.md

**Files:**
- Create: `CLAUDE.md`

- [ ] **Step 1: 创建 CLAUDE.md**

CLAUDE.md 包含三个部分：fork 说明、代码标注规则、技能参考。

```markdown
# cc-switch Fork 开发规范

本项目 fork 自 [farion1231/cc-switch](https://github.com/farion1231/cc-switch)，在此基础上有二次开发（定制化改造）。

## 代码标注规则

所有代码变更必须使用以下标记区分来源：

| 标记 | 含义 | 使用场景 |
|------|------|---------|
| `[Custom]` | 二次开发代码 | 新增/修改的自定义逻辑 |
| `[Upstream]` | 从上游合并 | 从上游合入的 bugfix/feat |
| `[Merge]` | 冲突处理区域 | 合并时人工处理的冲突 |
| `[Sync]` | 同步标记 | 同步过程中的辅助标记 |

### 标注位置规范

- **新文件**：文件头部添加注释 `// [Custom] 二次开发文件`
- **新函数/方法**：函数上方 `// [Custom] <说明>` 或 `// [Upstream] <sha> <说明>`
- **修改上游函数内部**：修改处添加 `// [Custom]` 内联注释
- **合并冲突区域**：用 `// [Merge-start]` / `// [Merge-end]` 包裹
- **配置/文档文件**：使用对应注释语法（`#`、`<!-- -->` 等）

### 合并策略

1. 二次开发代码优先保留
2. 上游 bugfix 且二开存在相同 bug → 应用相同修复
3. 上游 feat 不冲突则合入，冲突需确认
4. 上游 chore/docs 自动合入

## 技能

本仓库提供以下 Claude Code Skill：

### `/sync-upstream`

同步上游 `farion1231/cc-switch` 的最新代码到当前 `main` 分支。

流程：fetch upstream → 列出新提交 → 逐文件合并 → 标注代码 → 提交

### `/git-push`

推送前检查代码标注合规性，确保所有变更符合标注规则。

流程：扫描标注 → 检查冲突标记 → 补充缺失标注 → 提交推送
```

- [ ] **Step 2: 验证文件格式**

```bash
cd /home/0668001050/workspace/cc-switch && cat CLAUDE.md | head -5
```

Expected: 显示 CLAUDE.md 的前 5 行，无格式问题。

- [ ] **Step 3: 提交 CLAUDE.md**

```bash
cd /home/0668001050/workspace/cc-switch
git add CLAUDE.md
git commit -m "docs: add fork development guidelines with code annotation rules"
```

---

### Task 2: 创建辅助脚本 `scripts/sync-upstream.sh`

**Files:**
- Create: `scripts/sync-upstream.sh`

- [ ] **Step 1: 创建脚本目录**

```bash
mkdir -p /home/0668001050/workspace/cc-switch/scripts
```

- [ ] **Step 2: 创建 sync-upstream.sh**

```bash
#!/usr/bin/env bash
# [Custom] 二次开发: 上游同步辅助脚本
set -e

UPSTREAM_REMOTE="upstream"
SYNC_TAG="last-upstream-sync"
MAIN_BRANCH="main"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

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
    git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" --grep="fix" 2>/dev/null || echo "  无"
    echo "feat (新特性):"
    git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" --grep="feat" 2>/dev/null || echo "  无"
    echo "chore/docs/其他:"
    git log --oneline "$last_sync..$UPSTREAM_REMOTE/main" --grep="feat\|fix" --invert-grep 2>/dev/null || echo "  无"

    echo ""
    echo "=== 预测冲突文件 ==="
    local upstream_files
    upstream_files=$(git diff --name-only "$last_sync..$UPSTREAM_REMOTE/main" 2>/dev/null)
    local custom_files
    custom_files=$(git diff --name-only "$last_sync..HEAD" 2>/dev/null)
    if [ -n "$upstream_files" ] && [ -n "$custom_files" ]; then
        echo "$upstream_files" | sort | while read -r f; do
            if echo "$custom_files" | grep -q "^$f$"; then
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
```

- [ ] **Step 3: 设置可执行权限**

```bash
chmod +x /home/0668001050/workspace/cc-switch/scripts/sync-upstream.sh
```

- [ ] **Step 4: 验证脚本**

```bash
cd /home/0668001050/workspace/cc-switch && bash scripts/sync-upstream.sh --status
```

Expected: 显示分支、同步点、领先/落后计数。

- [ ] **Step 5: 提交**

```bash
cd /home/0668001050/workspace/cc-switch
git add scripts/sync-upstream.sh
git commit -m "feat: add upstream sync helper script"
```

---

### Task 3: 创建 `.claude/skills/sync-upstream.md`

**Files:**
- Create: `.claude/skills/sync-upstream.md`

- [ ] **Step 1: 创建 skills 目录**

```bash
mkdir -p /home/0668001050/workspace/cc-switch/.claude/skills
```

- [ ] **Step 2: 创建 sync-upstream.md**

````markdown
# `/sync-upstream` — 同步上游代码

同步 `farion1231/cc-switch` 的最新代码到当前 `main` 分支。
保留二次开发（`[Custom]`）代码，安全合入上游 fix/feat。

## 流程

### Phase 1: 环境检查

1. 确认当前在 `main` 分支
2. 确认工作区干净（无未提交改动）
3. 确认 `upstream` remote 已配置
4. 确认 `scripts/sync-upstream.sh` 存在

### Phase 2: 获取上游更新

1. 运行 `bash scripts/sync-upstream.sh --fetch`
2. 展示上游新提交清单（分类 fix/feat/chore）
3. 展示可能冲突的文件列表
4. 询问用户是否继续

### Phase 3: 用户确认

展示影响摘要：
- 新提交总数
- fix 数量和列表
- feat 数量和列表
- 冲突文件列表（如有）

等待用户输入：`继续` 或 `取消`

### Phase 4: 执行合并

1. 运行 `bash scripts/sync-upstream.sh --merge`

2. 如果存在冲突文件，逐文件处理：

   对于每个冲突文件：
   a. 读取文件内容，定位冲突区域（`<<<<<<< HEAD` / `=======` / `>>>>>>>`）
   b. 判断冲突区域性质：
      - 如果二开代码（`[Custom]`）在上游也有修改：
        - 保留二开逻辑
        - 检查上游修改是否是 bugfix
        - 如果是 bugfix 且二开代码存在相同 bug → 将修复逻辑应用到二开代码
        - 用 `// [Merge] 人工处理: <说明>` 注释包裹处理区域
      - 如果上游新增了二开没有的功能：
        - 保留上游代码，添加 `// [Upstream] <sha> <说明>` 注释
        - 在文件头更新标记
   c. `git add <文件>`

3. 对于无冲突但上游有改动的文件：
   - 读取文件，对每个被修改的代码块添加 `// [Upstream] <sha>` 注释
   - 如果文件头没有 `[Custom]` 标记且之前没有被修改过，添加 `// [Upstream]` 文件头注释

4. 运行 `git diff --cached --name-only` 确认所有文件已暂存

### Phase 5: 完成同步

1. 创建合并提交：

```bash
git commit -m "$(cat <<'EOF'
chore: sync upstream to $(date +%Y-%m-%d)

合并 upstream/main 到 main，包含:
$(bash scripts/sync-upstream.sh --fetch 2>&1 | tail -20)

[Sync] 同步上游代码
EOF
)"
```

2. 更新同步标签：

```bash
git tag -f last-upstream-sync upstream/main
```

3. 询问是否推送：

```bash
git push origin main --tags
```

4. 确认推送成功

### Phase 6: 后续处理

- 如果合并过程中有 `[Merge]` 标记的区域，建议在 `CLAUDE.md` 中记录本次合并的注意点
- 如遇到无法自动处理的复杂冲突，建议手动 review 最终结果
````

- [ ] **Step 3: 验证文件**

```bash
cd /home/0668001050/workspace/cc-switch && head -5 .claude/skills/sync-upstream.md
```

Expected: 显示 skill 文件前 5 行。

- [ ] **Step 4: 提交**

```bash
cd /home/0668001050/workspace/cc-switch
git add .claude/skills/sync-upstream.md
git commit -m "feat: add sync-upstream Claude Code skill"
```

---

### Task 4: 创建 `.claude/skills/git-push.md`

**Files:**
- Create: `.claude/skills/git-push.md`

- [ ] **Step 1: 创建 git-push.md**

```markdown
# `/git-push` — 合规检查后推送

推送代码前检查标注合规性，确保所有变更符合 CLAUDE.md 定义的标注规则。
防止未标注的二开代码或残留冲突标记被推送。

## 流程

### Phase 1: 检查状态

1. 运行 `git status` 展示变更概览
2. 运行 `git diff --stat` 展示文件级改动统计

### Phase 2: 注释合规扫描

1. 获取所有已修改/新增的文件列表：
   ```bash
   git diff --name-only HEAD
   ```

2. 对每个改动文件，检查：
   - 新增的代码行是否包含 `[Custom]`、`[Upstream]`、`[Merge]` 标记
   - 是否残留 git 冲突标记 `<<<<<<<` / `=======` / `>>>>>>>`
   - 文件头部是否有来源标记注释

3. 汇总扫描结果：
   - ✅ 合规文件列表
   - ⚠️ 缺少标注的文件（列出文件:行号）
   - ❌ 含有冲突残留的文件

### Phase 3: 修复标注

对于 ⚠️ 缺少标注的代码块：

1. 逐个定位文件中缺少标注的位置
2. 展示代码块上下文
3. 询问用户该代码块的来源：
   - `[Custom]` — 二次开发
   - `[Upstream]` — 从上游合并
   - `[Merge]` — 冲突处理
4. 根据用户回答添加相应的注释标记
5. 重新扫描确认

### Phase 4: 提交推送

1. 展示最终差异：`git diff`
2. 等待用户确认提交
3. 如果用户确认：
   ```bash
   git add -A
   git commit
   ```
4. 推送：
   ```bash
   git push origin main
   ```
5. 确认推送成功（检查返回状态）
```

- [ ] **Step 2: 验证文件**

```bash
cd /home/0668001050/workspace/cc-switch && head -5 .claude/skills/git-push.md
```

Expected: 显示 skill 文件前 5 行。

- [ ] **Step 3: 提交**

```bash
cd /home/0668001050/workspace/cc-switch
git add .claude/skills/git-push.md
git commit -m "feat: add git-push Claude Code skill with annotation compliance check"
```
