---
name: sync-upstream
description: 同步 farion1231/cc-switch 上游最新代码到当前 main 分支，保留 [Custom] 二次开发代码，安全合入上游 fix/feat，自动标注 [Upstream]/[Merge] 标记。当用户要求同步上游代码、合并 upstream、或需要从源仓库拉取最新变更时使用。
---

# sync-upstream — 同步上游代码

同步 `farion1231/cc-switch` 的最新代码到当前 `main` 分支。保留二次开发（`[Custom]`）代码，安全合入上游 fix/feat。

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
4. 提示用户进入确认阶段

### Phase 3: 用户确认

展示影响摘要：
- 新提交总数
- fix 数量和列表
- feat 数量和列表
- 冲突文件列表（如有）

等待用户输入：`继续` 或 `取消`

如果选择 `取消`：
- 运行 `git merge --abort`（如果合并已开始）
- 保持分支不变，不做任何更改

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
        - 用 `// [Merge-start] 人工处理: <说明>` / `// [Merge-end]` 注释包裹处理区域
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
- 遇到无法自动处理的复杂冲突，建议手动 review 最终结果
