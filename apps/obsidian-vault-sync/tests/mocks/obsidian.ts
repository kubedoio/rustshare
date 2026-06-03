// Mock Obsidian API for testing

export class TAbstractFile {
  path: string;
  name: string;
  constructor(path: string) {
    this.path = path;
    this.name = path.split('/').pop() || path;
  }
}

export class TFile extends TAbstractFile {
  extension: string;
  constructor(path: string) {
    super(path);
    this.extension = path.split('.').pop() || '';
  }
}

export class TFolder extends TAbstractFile {
  children: TAbstractFile[] = [];
}

export function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+/g, '/');
}

export class Vault {
  private files = new Map<string, TAbstractFile>();
  private contents = new Map<string, ArrayBuffer>();

  getFiles(): TFile[] {
    return Array.from(this.files.values()).filter(f => f instanceof TFile) as TFile[];
  }

  getAbstractFileByPath(path: string): TAbstractFile | null {
    return this.files.get(normalizePath(path)) || null;
  }

  readBinary(file: TFile): Promise<ArrayBuffer> {
    const content = this.contents.get(normalizePath(file.path));
    return Promise.resolve(content ?? new ArrayBuffer(0));
  }

  adapter = {
    mkdir: async (_path: string) => { /* no-op */ },
    writeBinary: async (path: string, data: ArrayBuffer) => {
      const normalized = normalizePath(path);
      this.contents.set(normalized, data);
      if (!this.files.has(normalized)) {
        this.files.set(normalized, new TFile(normalized));
      }
    },
  };

  delete(file: TAbstractFile): Promise<void> {
    this.files.delete(normalizePath(file.path));
    this.contents.delete(normalizePath(file.path));
    return Promise.resolve();
  }

  copy(file: TFile, newPath: string): Promise<void> {
    const normalized = normalizePath(newPath);
    const copy = new TFile(normalized);
    this.files.set(normalized, copy);
    const content = this.contents.get(normalizePath(file.path));
    if (content) {
      this.contents.set(normalized, content.slice(0));
    }
    return Promise.resolve();
  }

  // Test helpers
  addFile(path: string, content: string | ArrayBuffer): TFile {
    const normalized = normalizePath(path);
    const file = new TFile(normalized);
    this.files.set(normalized, file);
    if (typeof content === 'string') {
      const encoder = new TextEncoder();
      this.contents.set(normalized, encoder.encode(content).buffer);
    } else {
      this.contents.set(normalized, content);
    }
    return file;
  }

  clear(): void {
    this.files.clear();
    this.contents.clear();
  }
}

export class Plugin {}
export class Notice {}
export class App {}
export class PluginSettingTab {}
export class Setting {}
