/**
 * tauri-shim: 替代 @tauri-apps/plugin-process
 */
import { invoke } from './core';

export async function exit(code?: number): Promise<void> {
  try {
    await invoke('exit_app', { code: code ?? 0 });
  } catch {
    console.warn('[tauri-shim] exit_app failed');
  }
}

export async function relaunch(): Promise<void> {
  console.warn('[tauri-shim] relaunch is not available in server mode');
}

export function exitOnClose(): void {
  window.addEventListener('beforeunload', () => {
    // Best effort to notify backend
    navigator.sendBeacon(
      `http://localhost:${import.meta.env.VITE_CC_SWITCH_PORT || 10245}/api/exit_app`,
    );
  });
}
