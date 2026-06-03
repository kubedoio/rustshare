import { App, PluginSettingTab, Setting } from 'obsidian';
import type RustShareVaultSyncPlugin from '../main';

export class RustShareVaultSyncSettingTab extends PluginSettingTab {
  plugin: RustShareVaultSyncPlugin;

  constructor(app: App, plugin: RustShareVaultSyncPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();

    containerEl.createEl('h2', { text: 'RustShare Vault Sync Settings' });
    
    // Disclaimer
    containerEl.createEl('p', { 
      text: 'Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.',
      cls: 'setting-item-description'
    });

    new Setting(containerEl)
      .setName('RustShare URL')
      .setDesc('The URL of your RustShare instance')
      .addText(text => text
        .setPlaceholder('https://rustshare.io')
        .setValue(this.plugin.settings.rustshareUrl)
        .onChange(async (value) => {
          this.plugin.settings.rustshareUrl = value;
          await this.plugin.saveSettings();
        }));

    containerEl.createEl('p', {
      text: 'To connect, run the "Connect or create vault" command and follow the pairing instructions.',
      cls: 'setting-item-description'
    });

    new Setting(containerEl)
      .setName('Device name')
      .setDesc('A friendly name for this device')
      .addText(text => text
        .setPlaceholder('My MacBook')
        .setValue(this.plugin.settings.deviceName)
        .onChange(async (value) => {
          this.plugin.settings.deviceName = value;
          await this.plugin.saveSettings();
        }));

    new Setting(containerEl)
      .setName('Vault ID')
      .setDesc('The RustShare vault ID to sync with (leave empty to create new)')
      .addText(text => text
        .setPlaceholder('Optional vault ID')
        .setValue(this.plugin.settings.vaultId)
        .onChange(async (value) => {
          this.plugin.settings.vaultId = value;
          await this.plugin.saveSettings();
        }));

    new Setting(containerEl)
      .setName('Auto-sync interval')
      .setDesc('Minutes between automatic syncs (0 to disable)')
      .addText(text => text
        .setPlaceholder('0')
        .setValue(String(this.plugin.settings.autoSyncIntervalMinutes))
        .onChange(async (value) => {
          const num = parseInt(value, 10);
          this.plugin.settings.autoSyncIntervalMinutes = isNaN(num) ? 0 : Math.max(0, num);
          await this.plugin.saveSettings();
        }));

    containerEl.createEl('h3', { text: 'Advanced Sync Settings' });

    new Setting(containerEl)
      .setName('Save debounce (ms)')
      .setDesc('Milliseconds to wait after a file change before syncing')
      .addText(text => text
        .setPlaceholder('1500')
        .setValue(String(this.plugin.settings.debounceMs))
        .onChange(async (value) => {
          const num = parseInt(value, 10);
          this.plugin.settings.debounceMs = isNaN(num) ? 1500 : Math.max(100, num);
          await this.plugin.saveSettings();
        }));

    new Setting(containerEl)
      .setName('Max retries')
      .setDesc('Maximum retry attempts when offline')
      .addText(text => text
        .setPlaceholder('5')
        .setValue(String(this.plugin.settings.maxRetries))
        .onChange(async (value) => {
          const num = parseInt(value, 10);
          this.plugin.settings.maxRetries = isNaN(num) ? 5 : Math.max(0, num);
          await this.plugin.saveSettings();
        }));

    new Setting(containerEl)
      .setName('Retry base delay (ms)')
      .setDesc('Base delay between retries (doubles each attempt)')
      .addText(text => text
        .setPlaceholder('2000')
        .setValue(String(this.plugin.settings.retryBaseDelayMs))
        .onChange(async (value) => {
          const num = parseInt(value, 10);
          this.plugin.settings.retryBaseDelayMs = isNaN(num) ? 2000 : Math.max(500, num);
          await this.plugin.saveSettings();
        }));

    new Setting(containerEl)
      .setName('Log sync events')
      .setDesc('Enable in-memory sync logging for debugging')
      .addToggle(toggle => toggle
        .setValue(this.plugin.settings.logSyncEvents)
        .onChange(async (value) => {
          this.plugin.settings.logSyncEvents = value;
          await this.plugin.saveSettings();
        }));
  }
}
