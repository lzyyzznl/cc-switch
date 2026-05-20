/**
 * tauri-shim: 替代 @tauri-apps/plugin-dialog
 * 浏览器模式下使用原生 Web API
 */

export async function message(message: string): Promise<void> {
  alert(message);
}

export async function ask(message: string): Promise<boolean> {
  return window.confirm(message);
}

export async function confirm(message: string): Promise<boolean> {
  return window.confirm(message);
}

export async function open(options?: {
  multiple?: boolean;
  directory?: boolean;
  filters?: Array<{
    name: string;
    extensions: string[];
  }>;
}): Promise<string | string[] | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.style.display = 'none';
    if (options?.directory) {
      input.setAttribute('webkitdirectory', '');
    }
    if (options?.multiple) {
      input.multiple = true;
    }
    if (options?.filters && options.filters.length > 0) {
      input.accept = options.filters
        .flatMap((f) => f.extensions.map((e) => `.${e}`))
        .join(',');
    }
    input.addEventListener('change', () => {
      if (!input.files || input.files.length === 0) {
        resolve(null);
        return;
      }
      const files = Array.from(input.files).map((f) => f.name);
      resolve(options?.multiple ? files : files[0]);
    });
    input.click();
  });
}

export async function save(options?: {
  defaultPath?: string;
  filters?: Array<{ name: string; extensions: string[] }>;
}): Promise<string | null> {
  // Browser doesn't support save dialog directly
  // Return a prompt for the user to enter a path
  const name = prompt('Enter filename:', options?.defaultPath || 'config.json');
  return name;
}
