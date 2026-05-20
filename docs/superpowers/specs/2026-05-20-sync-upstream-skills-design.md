# Sync-Upstream & Git-Push Skills Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 创建两个 Claude Code Skill（`/sync-upstream` 和 `/git-push`），用于将上游 `farion1231/cc-switch` 的最新代码安全合入二次开发分支 `main`，并确保代码标注规范。

**Architecture:** 两个 Skill 文件 + 一个 CLAUDE.md 规范文件。`/sync-upstream` 负责从 upstream 拉取并合入最新代码，智能处理冲突且保护二次开发代码；`/git-push` 负责推送前检查代码标注合规性。两者共享同一套 `[Custom]` / `[Upstream]` / `[Merge]` 标注体系。

**Tech Stack:** Claude Code Skill (Markdown)、Git、Shell

---

## 文件结构

| 文件 | 类型 | 说明 |
|------|------|------|
| `.claude/skills/sync-upstream.md` | Create | `/sync-upstream` 技能 - 同步上游代码 |
| `.claude/skills/git-push.md` | Create | `/git-push` 技能 - 带标注检查的推送 |
| `CLAUDE.md` | Create | 项目根目录开发规范，含标注规则 |
| `scripts/sync-upstream.sh` | Create | 辅助脚本 - 处理机械性 git 操作 |

## 同步追踪机制

使用 git tag `last-upstream-sync` 记录上次同步到的上游 commit。

- 首次运行：tag 不存在，从 fork 点（`5315fa28`）开始
- 后续运行：从 tag 指向的 commit 到 `upstream/main` 之间找出新提交
- 同步完成后：`git tag -f last-upstream-sync upstream/main`

## 标注体系

| 标记 | 含义 | 使用场景 |
|------|------|---------|
| `[Custom]` | 二次开发代码 | 新增/修改的自定义逻辑 |
| `[Upstream]` | 从上游合并 | 从上游合入的 bugfix/feat |
| `[Merge]` | 冲突处理区域 | 合并时人工处理的冲突区域 |
| `[Sync]` | 同步标记 | 同步过程中的辅助标记 |

## 合并策略

按优先级：
1. **二次开发代码优先保留** — 冲突时以 `main` 分支的二开代码为准
2. **上游 bugfix 同款修复** — 如果二开代码存在相同 bug，应用相同修复逻辑
3. **上游 feat 选择性合入** — 不冲突的 feat 合并，冲突的 feat 需人工确认
4. **上游 chore/docs** — 自动合入

---

### Task 1: 创建 CLAUDE.md

**Files:**
- Create: `CLAUDE.md`

CLAUDE.md 包含 fork 开发规范、代码标注规则、以及 `/sync-upstream` 和 `/git-push` 技能的调用说明。

### Task 2: 创建辅助脚本 sync-upstream.sh

**Files:**
- Create: `scripts/sync-upstream.sh`

Shell 脚本，负责执行机械性 git 操作：
- 检查环境（分支、工作区、remote）
- `git fetch upstream`
- 查找 `last-upstream-sync` tag
- 列出上游新提交并分类
- 执行 `git merge upstream/main --no-ff --no-commit`
- 完成后 `git tag -f last-upstream-sync`

### Task 3: 创建 sync-upstream.md Skill

**Files:**
- Create: `.claude/skills/sync-upstream.md`

Skill 文件，引导用户执行完整的同步流程：

**Phase 1 - 准备**
1. 检查当前在 `main` 分支，工作区干净
2. 确认 `upstream` remote 存在
3. 检查 `last-upstream-sync` tag 是否存在，不存在则从 fork 点开始

**Phase 2 - 获取上游更新**
1. 运行 `scripts/sync-upstream.sh --fetch` 获取最新
2. 展示自上次同步以来的新提交清单
3. 按类型分类：`fix` / `feat` / `chore` / `docs`
4. 预览可能产生冲突的文件

**Phase 3 - 用户确认**
1. 展示影响范围
2. 询问是否继续

**Phase 4 - 执行合并**
1. 运行 `scripts/sync-upstream.sh --merge` 开始合并
2. 逐文件处理冲突：
   - 从冲突文件中提取三段式冲突区域
   - 判断哪些是二开代码（`[Custom]`）、哪些是上游代码（`[Upstream]`）
   - 按合并策略处理
   - 为合入的代码添加 `[Upstream]` 标注
   - 冲突区域用 `[Merge]` 包裹
   - `git add` 已处理的文件
3. 无冲突文件：为上游改动的代码添加 `[Upstream]` 标注

**Phase 5 - 完成**
1. `git commit` — 使用预设的合并提交模板
2. `git tag -f last-upstream-sync upstream/main`
3. 询问是否推送到 `origin`

### Task 4: 创建 git-push.md Skill

**Files:**
- Create: `.claude/skills/git-push.md`

Skill 文件，引导用户执行带注释检查的推送：

**Phase 1 - 检查状态**
1. `git status` 展示所有变更
2. `git diff` 展示详细差异

**Phase 2 - 注释合规扫描**
1. 提取所有新增/修改的代码行
2. 检查是否包含 `[Custom]`、`[Upstream]`、`[Merge]` 标注
3. 检查是否残留 `<<<<<<<` / `=======` / `>>>>>>>` 冲突标记
4. 列出所有缺少标注的代码位置（文件:行号）

**Phase 3 - 修复标注**
1. 逐个展示缺失标注的代码块
2. 询问添加合适的标注
3. 完成后重新扫描确认

**Phase 4 - 提交推送**
1. 展示最终差异
2. 询问是否提交
3. `git add` + `git commit`
4. `git push`
5. 确认推送成功
