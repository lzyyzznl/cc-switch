/**
 * tauri-shim: 替代 @tauri-apps/api/core
 * 通过 HTTP fetch 调用 Rust 后端 API
 */
const BASE = `http://localhost:${import.meta.env.VITE_CC_SWITCH_PORT || 10245}`;

/**
 * Tauri Resource — stub for HTTP mode.
 * In Tauri, Resource wraps a numeric resource ID for IPC lifecycle management.
 * In HTTP mode, all resources are managed server-side, so this is a no-op.
 */
export class Resource {
  constructor(public rid: number) {}
}

/**
 * Tauri Channel — stub for HTTP mode.
 * In Tauri, Channel enables callback-based streaming from Rust to JS.
 * In HTTP mode, SSE is used instead.
 */
export class Channel {
  constructor() {}
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  onmessage = (_msg: unknown) => {};
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const resp = await fetch(`${BASE}/api/${cmd}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: args ? JSON.stringify(args) : '{}',
  });
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ error: resp.statusText }));
    throw body;
  }
  return resp.json() as Promise<T>;
}

export async function transformCallback<T>(val: T): Promise<T> {
  return val;
}
