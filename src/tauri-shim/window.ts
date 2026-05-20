/**
 * tauri-shim: 替代 @tauri-apps/api/window
 * 浏览器模式下窗口 API 无操作
 */
export function getCurrentWindow() {
  return createNoopWindow();
}

function createNoopWindow() {
  return {
    setTitle: async (_title: string) => {},
    setSize: async (_size: { width: number; height: number }) => {},
    show: async () => {},
    hide: async () => {},
    setFocus: async () => {},
    setSkipTaskbar: async (_skip: boolean) => {},
    setDecorations: async (_decorations: boolean) => {},
    innerSize: async () => ({ width: 0, height: 0 }),
    minimize: async () => {},
    unminimize: async () => {},
    close: async () => {},
    destroy: async () => {},
    setFullscreen: async (_fullscreen: boolean) => {},
    isFullscreen: async () => false,
    isMaximized: async () => false,
    maximize: async () => {},
    unmaximize: async () => {},
    isVisible: async () => true,
    onResized: async (_cb: () => void) => {},
    onMoved: async (_cb: () => void) => {},
    onCloseRequested: async (_cb: () => void) => {},
    theme: () => null,
  };
}
