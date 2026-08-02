import { invoke } from '@tauri-apps/api/core';

export function closeWindow(): Promise<void> {
  return invoke('close_window');
}

export function minimizeWindow(): Promise<void> {
  return invoke('minimize_window');
}

export function toggleMaximizeWindow(): Promise<void> {
  return invoke('toggle_maximize_window');
}
