/**
 * tauri-shim: 替代 @tauri-apps/api/app
 */
import { invoke } from './core';

export async function getVersion(): Promise<string> {
  try {
    return await invoke<string>('get_tool_versions');
  } catch {
    return '3.15.0';
  }
}

export async function getName(): Promise<string> {
  return 'CC Switch';
}
