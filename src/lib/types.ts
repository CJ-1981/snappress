export type CaptureKind = 'image' | 'video' | 'gif';

export type CaptureOrigin = 'capture' | 'scroll' | 'record' | 'edit';

export interface CaptureItem {
  id: string;
  kind: CaptureKind;
  path: string;
  createdAt: string;
  width: number;
  height: number;
  sizeBytes: number;
  durationMs?: number;
  origin?: CaptureOrigin;
}

export interface DisplayInfo {
  id: number;
  name: string;
  width: number;
  height: number;
  scaleFactor: number;
  isPrimary: boolean;
}

export type RecordFormat = 'mp4';

export interface AppSettings {
  recordingFps: number;
  gifFps: number;
  gifWidth: number;
  scrollDelayMs: number;
  scrollStepTicks: number;
  maxScrollHeight: number;
  /** Days after which library items are removed (0 = never). */
  autoDeleteDays: number;
  /** Global shortcuts in the shortcut plugin's format; empty = disabled. */
  hotkeyImage: string;
  hotkeyScroll: string;
  hotkeyRecord: string;
  hotkeyStop: string;
  theme: 'dark';
}

/** What saving settings returns: the saved values plus shortcut warnings. */
export interface SavedSettings {
  settings: AppSettings;
  /** Shortcuts that could not be registered; defaults are used instead. */
  hotkeyWarnings: string[];
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type CaptureMode = 'image' | 'scroll' | 'record';

export interface PendingOverlay {
  mode: CaptureMode;
  displayId: number | null;
}
