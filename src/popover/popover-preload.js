'use strict';
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('usageAPI', {
  onUsage: (cb) => ipcRenderer.on('usage', (_e, data) => cb(data)),
  onSettings: (cb) => ipcRenderer.on('settings', (_e, data) => cb(data)),
  onResetView: (cb) => ipcRenderer.on('reset-view', () => cb()),
  getSettings: () => ipcRenderer.invoke('get-settings'),
  setSettings: (patch) => ipcRenderer.invoke('set-settings', patch),
  refresh: () => ipcRenderer.invoke('refresh'),
  openClaude: () => ipcRenderer.invoke('open-claude'),
  resize: (height) => ipcRenderer.send('resize-popover', height),
});
