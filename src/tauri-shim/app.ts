/**
 * tauri-shim: 替代 @tauri-apps/api/app
 */

export async function getVersion(): Promise<string> {
  // [Custom] tauri-shim 中 invoke('get_tool_versions') 返回的是工具版本数组而非应用版本，
  // 直接返回固定版本号，实际版本由 Rust 后端管理
  return '3.15.0';
}

export async function getName(): Promise<string> {
  return 'CC Switch';
}
