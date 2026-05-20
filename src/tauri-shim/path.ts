/**
 * tauri-shim: 替代 @tauri-apps/api/path
 * 在浏览器模式下返回静态值
 */
export async function homeDir(): Promise<string> {
  return '/home';
}

export async function join(...paths: string[]): Promise<string> {
  return paths.join('/');
}
