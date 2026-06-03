// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

import { Plugin, setIcon } from 'obsidian';

type SyncStatus = 'disconnected' | 'connected' | 'syncing' | 'synced' | 'conflict' | 'error' | 'offline';

export class StatusBar {
  private statusBarItemEl: HTMLElement;
  private currentStatus: SyncStatus = 'disconnected';

  constructor(plugin: Plugin) {
    this.statusBarItemEl = plugin.addStatusBarItem();
    this.statusBarItemEl.addClass('rustshare-sync-status');
    this.updateStatus('disconnected');
  }

  updateStatus(status: SyncStatus, message?: string): void {
    this.currentStatus = status;
    this.statusBarItemEl.empty();

    const iconEl = this.statusBarItemEl.createEl('span', { cls: 'rustshare-sync-status-icon' });
    const textEl = this.statusBarItemEl.createEl('span', { text: message || this.statusText(status) });

    this.statusBarItemEl.removeClass(
      'rustshare-sync-status--disconnected',
      'rustshare-sync-status--connected',
      'rustshare-sync-status--syncing',
      'rustshare-sync-status--synced',
      'rustshare-sync-status--conflict',
      'rustshare-sync-status--error',
      'rustshare-sync-status--offline'
    );
    this.statusBarItemEl.addClass(`rustshare-sync-status--${status}`);
  }

  private statusText(status: SyncStatus): string {
    switch (status) {
      case 'disconnected': return 'RustShare: Disconnected';
      case 'connected': return 'RustShare: Connected';
      case 'syncing': return 'RustShare: Syncing...';
      case 'synced': return 'RustShare: Synced';
      case 'conflict': return 'RustShare: Conflict';
      case 'error': return 'RustShare: Error';
      case 'offline': return 'RustShare: Offline';
    }
  }
}
