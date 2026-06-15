// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

import { Plugin, Notice, TFile } from 'obsidian';
import { RustShareVaultSyncSettings, DEFAULT_SETTINGS, validateSettings } from './settings';
import { RustShareVaultSyncSettingTab } from './ui/settings-tab';
import { StatusBar } from './ui/status-bar';
import { RustShareAPI } from './api';
import { SyncEngine } from './sync';
import { SyncState, createEmptySyncState, migrateSyncState, pruneTombstones } from './state';
import { SyncQueue, SyncOperation } from './sync-queue';
import { syncLog } from './sync-log';
import { generateDeviceId, detectCloudSyncFolder, shouldIgnorePath, isValidUuid, loadDesktopAuthToken } from './utils';

export default class RustShareVaultSyncPlugin extends Plugin {
  declare settings: RustShareVaultSyncSettings;
  statusBar: StatusBar;
  syncState: SyncState | null = null;
  private syncQueue: SyncQueue;
  private syncInterval: number | null = null;
  private isSyncing = false;

  async onload() {
    await this.loadSettings();

    // Generate device name if not set
    if (!this.settings.deviceName) {
      this.settings.deviceName = `${this.app.vault.getName()} - ${navigator.platform}`;
      await this.saveSettings();
    }

    // Status bar
    this.statusBar = new StatusBar(this);

    // Sync queue
    this.syncQueue = new SyncQueue(
      {
        debounceMs: this.settings.debounceMs,
        maxRetries: this.settings.maxRetries,
        retryBaseDelayMs: this.settings.retryBaseDelayMs,
        retryMaxDelayMs: 60000,
      },
      async (ops: SyncOperation[]) => {
        if (!this.settings.vaultId) {
          syncLog.debug('Sync queue: no vault configured, skipping');
          return;
        }
        if (this.isSyncing) {
          syncLog.debug('Sync queue: full sync in progress, skipping incremental');
          return;
        }
        await this.runIncrementalSync(ops);
      }
    );
    this.syncQueue.start();

    // Ribbon icon
    this.addRibbonIcon('cloud', 'RustShare Vault Sync', (evt: MouseEvent) => {
      this.runManualSync();
    });

    // Commands
    this.addCommand({
      id: 'rustshare-sync-vault',
      name: 'Sync vault to RustShare',
      callback: () => this.runManualSync(),
    });

    this.addCommand({
      id: 'rustshare-connect-vault',
      name: 'Connect or create vault',
      callback: () => this.connectVault(),
    });

    // Settings tab
    this.addSettingTab(new RustShareVaultSyncSettingTab(this.app, this));

    // Auto-sync interval
    if (this.settings.autoSyncIntervalMinutes > 0) {
      this.startAutoSync();
    }

    // File event listeners
    this.registerEvent(this.app.vault.on('create', (file) => {
      if (file instanceof TFile && !shouldIgnorePath(file.path)) {
        syncLog.debug('Event: create', file.path);
        this.syncQueue.add({ path: file.path, type: 'create' });
      }
    }));

    this.registerEvent(this.app.vault.on('delete', (file) => {
      if (file instanceof TFile && !shouldIgnorePath(file.path)) {
        syncLog.debug('Event: delete', file.path);
        this.syncQueue.add({ path: file.path, type: 'delete' });
      }
    }));

    this.registerEvent(this.app.vault.on('rename', (file, oldPath) => {
      if (file instanceof TFile && !shouldIgnorePath(file.path)) {
        syncLog.debug('Event: rename', `${oldPath} -> ${file.path}`);
        this.syncQueue.add({ path: file.path, type: 'rename', oldPath });
      }
    }));

    this.registerEvent(this.app.vault.on('modify', (file) => {
      if (file instanceof TFile && !shouldIgnorePath(file.path)) {
        syncLog.debug('Event: modify', file.path);
        this.syncQueue.add({ path: file.path, type: 'modify' });
      }
    }));

    // Network online/offline detection
    this.registerDomEvent(window, 'online', () => {
      syncLog.info('Network online detected');
      this.syncQueue.setOnline(true);
    });
    this.registerDomEvent(window, 'offline', () => {
      syncLog.info('Network offline detected');
      this.syncQueue.setOnline(false);
    });

    // Check for double-sync warning
    const vaultPath = (this.app.vault.adapter as any).getBasePath?.() || '';
    const cloudFolder = detectCloudSyncFolder(vaultPath);
    if (cloudFolder) {
      new Notice(
        `Warning: This vault appears to be inside ${cloudFolder}. Using multiple sync engines on the same vault can cause conflicts.`,
        10000
      );
    }
  }

  onunload() {
    this.stopAutoSync();
    this.syncQueue.stop();
  }

  async loadSettings() {
    const data = await this.loadData();
    this.settings = Object.assign({}, DEFAULT_SETTINGS, data);
    if (data?.syncState) {
      this.syncState = migrateSyncState(data.syncState);
    }
  }

  async saveSettings() {
    await this.saveData({ ...this.settings, syncState: this.syncState });
  }

  private async connectVault(): Promise<void> {
    // Validate URL
    if (!this.settings.rustshareUrl) {
      new Notice('Please enter your RustShare URL in settings first.', 10000);
      return;
    }

    let api: RustShareAPI;
    try {
      api = new RustShareAPI(this.settings.rustshareUrl, ''); // no token yet
    } catch (e: any) {
      const message = e?.message || String(e);
      console.error('RustShare Vault Sync: invalid URL', e);
      new Notice(`Invalid RustShare URL: ${message}`, 10000);
      this.statusBar.updateStatus('error', 'Invalid URL');
      return;
    }

    try {
      // If rustshare-desktop is already authenticated, reuse its token so the
      // user does not have to pair Obsidian separately.
      const desktopToken = loadDesktopAuthToken();
      if (desktopToken) {
        console.log('RustShare Vault Sync: reusing desktop auth token');
        new Notice('Found RustShare desktop auth token. Skipping device pairing.', 5000);
        this.settings.authToken = desktopToken;
        await this.saveSettings();
        await this.finishConnect(desktopToken);
        return;
      }

      this.statusBar.updateStatus('syncing', 'Requesting device pairing...');
      console.log('RustShare Vault Sync: requesting device pairing');

      // Step 1: Request device pairing
      const pairing = await api.requestDevicePairing();
      console.log('RustShare Vault Sync: pairing requested', pairing);

      // Step 2: Show pairing code to user
      const baseUrl = this.settings.rustshareUrl.replace(/\/$/, '');
      const manualApprovalUrl = `${baseUrl}/device`;
      const displayCode = pairing.user_code;

      // Copy the code to the clipboard so the user can paste it on the web page.
      try {
        await navigator.clipboard.writeText(displayCode);
      } catch (clipErr) {
        console.warn('RustShare Vault Sync: could not copy pairing code', clipErr);
      }

      new Notice(
        `Pairing code: ${displayCode} (copied). Go to ${manualApprovalUrl} on an already authenticated device and enter the code.`,
        30000
      );

      this.statusBar.updateStatus('syncing', `Waiting for approval (code: ${displayCode})...`);

      // Step 3: Poll for approval
      let token: string | null = null;
      const startTime = Date.now();
      const maxWaitMs = pairing.expires_in * 1000;
      let pollIntervalMs = 20000;

      while (Date.now() - startTime < maxWaitMs) {
        await new Promise(resolve => setTimeout(resolve, pollIntervalMs));

        try {
          const poll = await api.pollDevicePairing(pairing.device_code);
          console.log('RustShare Vault Sync: poll status', poll.status);

          if (poll.status === 'approved') {
            token = poll.token;
            break;
          } else if (poll.status === 'expired') {
            throw new Error('Pairing code expired. Please try again.');
          }
          // status === 'pending' → continue polling
        } catch (pollErr: any) {
          if (pollErr?.status === 429) {
            const retryAfter = typeof pollErr?.retry_after === 'number' ? pollErr.retry_after : 5;
            console.warn(`RustShare Vault Sync: poll rate-limited, retrying after ${retryAfter}s`);
            new Notice(`Server is busy. Retrying in ${retryAfter} seconds...`, 3000);
            pollIntervalMs = Math.min(Math.max(retryAfter * 1000, 20000), 30000);
            continue;
          }
          throw pollErr;
        }
      }

      if (!token) {
        throw new Error('Pairing timed out. Please try again.');
      }

      console.log('RustShare Vault Sync: device approved, registering device');

      // Step 4: Store token
      this.settings.authToken = token;
      await this.saveSettings();

      new Notice('Device paired successfully!');

      await this.finishConnect(token);
    } catch (e: any) {
      const message = e?.message || String(e);
      console.error('RustShare Vault Sync: connect failed', e);
      this.statusBar.updateStatus('error', `Connect failed: ${message}`);
      new Notice(`Failed to connect: ${message}`, 10000);
    }
  }

  private async finishConnect(token: string): Promise<void> {
    const authedApi = new RustShareAPI(this.settings.rustshareUrl, token);

    // Register device
    let deviceId: string;
    try {
      const resp = await authedApi.registerDevice(this.settings.deviceName, 'obsidian_plugin', '0.1.0');
      deviceId = resp.id;
      this.settings.deviceId = deviceId;
      await this.saveSettings();
      console.log('RustShare Vault Sync: device registered', deviceId);
    } catch (e: any) {
      const isNetworkError = e instanceof TypeError ||
        /fetch|network|Failed to fetch|net::ERR/i.test(e?.message || '');

      if (this.settings.deviceId) {
        deviceId = this.settings.deviceId;
        if (isNetworkError) {
          console.info('Using cached device ID due to network error during registration:', e);
        } else {
          console.warn('Using cached device ID, registration failed:', e);
        }
      } else {
        throw new Error(`Failed to register device: ${e.message || e}`);
      }
    }

    // Create or use existing vault
    let vaultId = this.settings.vaultId;
    if (vaultId && !isValidUuid(vaultId)) {
      console.warn('RustShare Vault Sync: ignoring invalid stored vaultId:', vaultId);
      new Notice(
        `Stored Vault ID "${vaultId}" is not a valid UUID. A new vault will be created.`,
        10000
      );
      vaultId = '';
    }
    if (!vaultId) {
      const vault = await authedApi.createVault({
        name: this.app.vault.getName(),
        adapter: 'ObsidianVault',
        client_vault_id: undefined,
        device_id: this.settings.deviceId,
      });
      vaultId = vault.id;
      this.settings.vaultId = vaultId;
      await this.saveSettings();
      new Notice(`Created vault: ${vault.name}`);
    } else {
      const vault = await authedApi.getVault(vaultId);
      new Notice(`Connected to vault: ${vault.name}`);
    }

    // Initialize sync state
    if (!this.syncState || this.syncState.vault_id !== vaultId || this.syncState.device_id !== deviceId) {
      this.syncState = createEmptySyncState(vaultId, deviceId, this.settings.deviceName);
    }

    this.statusBar.updateStatus('connected');
  }

  private async runManualSync(): Promise<void> {
    if (this.isSyncing) {
      new Notice('Sync already in progress');
      return;
    }
    // Full sync handles all files; no need to flush incremental queue
    this.isSyncing = true;
    try {
      if (!this.settings.vaultId) {
        new Notice('No vault configured. Run "Connect or create vault" first.');
        this.isSyncing = false;
        return;
      }

      if (!this.syncState) {
        this.syncState = createEmptySyncState(
          this.settings.vaultId,
          this.settings.deviceId,
          this.settings.deviceName
        );
      }

      const api = new RustShareAPI(this.settings.rustshareUrl, this.settings.authToken);
      const engine = new SyncEngine(this.app.vault, api, this.syncState, this.settings.deviceName);

      try {
        this.statusBar.updateStatus('syncing', 'Syncing...');
        const result = await engine.sync();

        await this.saveSettings();
        if (this.syncState) {
          pruneTombstones(this.syncState);
        }

        const msg = `Sync complete: ${result.uploaded}↑ ${result.downloaded}↓ ${result.deleted}🗑 ${result.conflicts}⚠`;
        if (result.errors.length > 0) {
          this.statusBar.updateStatus('error', `${msg} (${result.errors.length} errors)`);
          console.error('Sync errors:', result.errors);
        } else if (result.conflicts > 0) {
          this.statusBar.updateStatus('conflict', msg);
        } else {
          this.statusBar.updateStatus('synced', msg);
        }
        new Notice(msg, 4000);
        this.syncQueue.setOnline(true);
      } catch (e) {
        this.statusBar.updateStatus('error', `Sync failed: ${e}`);
        new Notice(`Sync failed: ${e}`);
      }
    } finally {
      this.isSyncing = false;
    }
  }

  private async runIncrementalSync(ops: SyncOperation[]): Promise<void> {
    if (!this.settings.vaultId || !this.syncState) {
      syncLog.debug('Incremental sync: no vault or state');
      return;
    }
    this.isSyncing = true;
    try {
      const api = new RustShareAPI(this.settings.rustshareUrl, this.settings.authToken);
      const engine = new SyncEngine(this.app.vault, api, this.syncState, this.settings.deviceName);

      this.statusBar.updateStatus('syncing', 'Syncing...');
      const result = await engine.syncIncremental(ops);
      await this.saveSettings();
      if (this.syncState) {
        pruneTombstones(this.syncState);
      }

      const msg = `Incremental sync: ${result.uploaded}↑ ${result.downloaded}↓ ${result.deleted}🗑 ${result.conflicts}⚠`;
      if (result.errors.length > 0) {
        this.statusBar.updateStatus('error', `${msg} (${result.errors.length} errors)`);
        console.error('Incremental sync errors:', result.errors);
      } else if (result.conflicts > 0) {
        this.statusBar.updateStatus('conflict', msg);
      } else {
        this.statusBar.updateStatus('synced', msg);
      }
      if (this.settings.logSyncEvents) {
        syncLog.info(msg);
      }
      this.syncQueue.setOnline(true);
    } catch (e) {
      this.statusBar.updateStatus('error', `Incremental sync failed: ${e}`);
      syncLog.error('Incremental sync failed', undefined, String(e));
      throw e;
    } finally {
      this.isSyncing = false;
    }
  }

  private startAutoSync(): void {
    if (this.syncInterval) return;
    const ms = this.settings.autoSyncIntervalMinutes * 60 * 1000;
    this.syncInterval = window.setInterval(() => this.runManualSync(), ms);
  }

  private stopAutoSync(): void {
    if (this.syncInterval) {
      window.clearInterval(this.syncInterval);
      this.syncInterval = null;
    }
  }
}
