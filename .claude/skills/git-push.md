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
   git commit -m "chore: compliance check passed push"
   ```
4. 推送：
   ```bash
   git push origin $(git branch --show-current)
   ```
5. 确认推送成功（检查返回状态）
