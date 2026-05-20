#!/usr/bin/env bash
# [Custom] 二次开发: 开发环境启动脚本
set -e

BACKEND_PORT="${CC_SWITCH_PORT:-10245}"
FRONTEND_PORT=3000
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

kill_port() {
    local port=$1
    local pids
    pids=$(lsof -ti tcp:"$port" 2>/dev/null) || return 0
    echo "  → 杀死进程 PID $(echo "$pids" | tr '\n' ' ')（占用端口 $port）"
    echo "$pids" | xargs -r kill -9 2>/dev/null || true
    sleep 1
}

echo "=== cc-switch 开发环境启动 ==="
echo ""

# 先清理两个端口
kill_port "$BACKEND_PORT"
kill_port "$FRONTEND_PORT"

# ── 后端 ──
echo "[1/2] 启动后端 (port $BACKEND_PORT)..."
cd "$PROJECT_ROOT/src-tauri"
CC_SWITCH_PORT="$BACKEND_PORT" \
    CC_SWITCH_DIST_DIR="$PROJECT_ROOT/dist" \
    cargo run --features server_only --bin cc-switch-server &
BACKEND_PID=$!

# ── 前端 ──
echo "[2/2] 启动前端开发服务器 (port $FRONTEND_PORT)..."
cd "$PROJECT_ROOT"
pnpm dev:renderer &
FRONTEND_PID=$!

echo ""
echo "========================================"
echo "后端: http://localhost:$BACKEND_PORT"
echo "前端: http://localhost:$FRONTEND_PORT"
echo "按 Ctrl+C 停止所有服务"
echo "========================================"

trap 'echo ""; echo "正在停止..."; kill $BACKEND_PID $FRONTEND_PID 2>/dev/null; exit 0' INT TERM
wait
