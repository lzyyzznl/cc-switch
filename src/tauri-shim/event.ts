/**
 * tauri-shim: 替代 @tauri-apps/api/event
 * 通过 SSE (EventSource) 接收后端事件推送
 */
export type UnlistenFn = () => void;

const BASE = `http://localhost:${import.meta.env.VITE_CC_SWITCH_PORT || 10245}`;
const activeSources = new Map<string, EventSource>();

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  let source = activeSources.get('default');
  if (!source) {
    source = new EventSource(`${BASE}/events`);
    source.onerror = () => {
      // SSE connection error - will auto-reconnect
      console.debug('[tauri-shim] SSE connection error, will retry');
    };
    activeSources.set('default', source);
  }

  const wrappedHandler = (e: MessageEvent) => {
    try {
      handler({ payload: JSON.parse(e.data) as T });
    } catch {
      handler({ payload: e.data as unknown as T });
    }
  };

  source.addEventListener(event, wrappedHandler);
  return () => {
    source?.removeEventListener(event, wrappedHandler);
    // Don't close the source - other listeners may be using it
  };
}
