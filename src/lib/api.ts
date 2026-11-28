import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppSettings,
  CaptureItem,
  CaptureMode,
  DisplayInfo,
  PendingOverlay,
  Rect,
  SavedSettings,
} from './types';

export const api = {
  // capture / overlay
  startRegionCapture: (mode: CaptureMode, displayId?: number | null) =>
    invoke<void>('start_region_capture', { mode, displayId: displayId ?? null }),
  cancelCapture: () => invoke<void>('cancel_capture'),
  overlayReady: () => invoke<PendingOverlay>('overlay_ready'),
  captureFullScreen: (displayId?: number | null) =>
    invoke<CaptureItem>('capture_full_screen', { displayId: displayId ?? null }),
  captureRegion: (displayId: number | null, rect: Rect) =>
    invoke<CaptureItem>('capture_region', {
      displayId,
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    }),
  startScrollCapture: (
    displayId: number | null,
    rect: Rect,
    opts: { delayMs: number; stepTicks: number; maxHeight: number },
  ) =>
    invoke<void>('start_scroll_capture', {
      displayId,
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      delayMs: opts.delayMs,
      stepTicks: opts.stepTicks,
      maxHeight: opts.maxHeight,
    }),
  cancelScrollCapture: () => invoke<void>('cancel_scroll_capture'),

  // recording
  startRecording: (displayId: number | null, rect: Rect) =>
    invoke<void>('start_recording', {
      displayId,
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    }),
  stopRecording: () => invoke<void>('stop_recording'),
  pauseRecording: (paused: boolean) => invoke<void>('pause_recording', { paused }),

  // library
  listLibrary: () => invoke<CaptureItem[]>('list_library'),
  deleteItem: (id: string) => invoke<void>('delete_item', { id }),
  deleteAllItems: () => invoke<number>('delete_all_items'),
  openPath: (path: string, reveal?: boolean) =>
    invoke<void>('open_path', { path, reveal: reveal ?? null }),
  getLibraryDir: () => invoke<string>('get_library_dir'),
  copyImageToClipboard: (path: string) => invoke<void>('copy_image_to_clipboard', { path }),
  // Each copies first and deletes only once the image is on the clipboard.
  copyImageAndDelete: (dataUrl: string, id: string) =>
    invoke<void>('copy_image_and_delete', { data: dataUrl, id }),
  copyCaptureAndDelete: (id: string) => invoke<void>('copy_capture_and_delete', { id }),
  exportGif: (id: string) => invoke<void>('export_gif', { id }),
  saveEdited: (dataUrl: string, format: 'png' | 'jpeg') =>
    invoke<CaptureItem>('save_edited', { data: dataUrl, format }),
  readImageBase64: (path: string) => invoke<string>('read_image_base64', { path }),

  // system
  listDisplays: () => invoke<DisplayInfo[]>('list_displays'),
  screenPermission: () => invoke<boolean>('screen_permission_status'),
  requestScreenPermission: () => invoke<boolean>('request_screen_permission'),
  openScreenSettings: () => invoke<void>('open_screen_settings'),
  // While held, the global shortcuts are unregistered so a press reaches the
  // shortcut fields in the settings dialog instead of triggering the action.
  holdHotkeys: (hold: boolean) => invoke<void>('hold_hotkeys', { hold }),
  getSettings: () => invoke<AppSettings>('get_settings'),
  setSettings: (settings: AppSettings) => invoke<SavedSettings>('set_settings', { settings }),
  copyDiagnostics: () => invoke<string>('copy_diagnostics'),
  openLogFolder: () => invoke<void>('open_log_folder'),
};

export function onEvent<T>(name: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => cb(e.payload));
}
