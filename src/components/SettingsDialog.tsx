import { X } from 'lucide-react';
import { useEffect, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from 'react';
import { api } from '../lib/api';
import type { AppSettings, SavedSettings } from '../lib/types';

interface Props {
  open: boolean;
  settings: AppSettings;
  onClose: () => void;
  onSaved: (s: SavedSettings) => void;
}

const selectCls =
  'w-full rounded-md border border-zinc-800 bg-zinc-950 px-2.5 py-2 text-sm text-zinc-200 focus:border-emerald-500/60 focus:outline-none';

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-medium text-zinc-400">{label}</span>
      {children}
    </label>
  );
}

export default function SettingsDialog({ open, settings, onClose, onSaved }: Props) {
  if (!open) return null;
  // Remount on every open so the draft always starts from the saved settings
  // without needing a state-sync effect.
  return <SettingsDialogInner key={settings.theme} settings={settings} onClose={onClose} onSaved={onSaved} />;
}

function SettingsDialogInner({ settings, onClose, onSaved }: Omit<Props, 'open'>) {
  const [draft, setDraft] = useState<AppSettings>(settings);

  // The shortcuts are inactive while this dialog is open (the fields capture
  // key presses), and are re-applied from the settings when it closes.
  useEffect(() => {
    api.holdHotkeys(true).catch(() => {});
    return () => {
      api.holdHotkeys(false).catch(() => {});
    };
  }, []);

  const set = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));

  const [diagnostics, setDiagnostics] = useState<'idle' | 'copied' | 'failed'>('idle');

  const copyDiagnostics = async () => {
    try {
      await api.copyDiagnostics();
      setDiagnostics('copied');
    } catch {
      setDiagnostics('failed');
    }
  };

  const openLogFolder = async () => {
    try {
      await api.openLogFolder();
    } catch {
      setDiagnostics('failed');
    }
  };

  const save = async () => {
    try {
      const s = await api.setSettings(draft);
      onSaved(s);
      onClose();
    } catch {
      /* dialog stays open on failure */
    }
  };

  // Which shortcuts are set twice in this draft; the backend would only
  // register the first of them, so say so before saving.
  const hotkeyFields: [keyof AppSettings & `hotkey${string}`, string, string][] = [
    ['hotkeyImage', 'Region capture', 'Opens the capture overlay.'],
    ['hotkeyScroll', 'Scrolling capture', 'Starts a scrolling capture.'],
    ['hotkeyRecord', 'Record screen', 'Starts a screen recording.'],
    ['hotkeyStop', 'Stop recording', 'Stops the running recording.'],
  ];
  const hotkeyUseCounts = new Map<string, string[]>();
  for (const [key, label] of hotkeyFields) {
    const value = draft[key] as string;
    if (value.trim()) {
      hotkeyUseCounts.set(value.trim(), [...(hotkeyUseCounts.get(value.trim()) ?? []), label]);
    }
  }

  return (
    <div
      className="fixed inset-0 z-[70] flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      onClick={onClose}
    >
      <div
        className="flex max-h-[calc(100vh-2rem)] w-full max-w-md flex-col overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 pb-4 pt-5">
          <h2 className="text-base font-semibold">Settings</h2>
          <button
            onClick={onClose}
            aria-label="Close settings"
            className="rounded-md p-1.5 text-zinc-400 transition hover:bg-zinc-800 hover:text-zinc-200"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        {/* Only this part scrolls, so the actions stay on screen. */}
        <div className="min-h-0 flex-1 overflow-y-auto px-6">
          <div className="space-y-4 text-sm">
          <Field label="Recording FPS">
            <select
              value={draft.recordingFps}
              onChange={(e) => set('recordingFps', Number(e.target.value))}
              className={selectCls}
            >
              {[15, 24, 30].map((v) => (
                <option key={v} value={v}>
                  {v} fps
                </option>
              ))}
            </select>
          </Field>
          <Field label="GIF FPS (used on export)">
            <select
              value={draft.gifFps}
              onChange={(e) => set('gifFps', Number(e.target.value))}
              className={selectCls}
            >
              {[8, 10, 12, 15].map((v) => (
                <option key={v} value={v}>
                  {v} fps
                </option>
              ))}
            </select>
          </Field>
          <Field label="GIF width (used on export)">
            <select
              value={draft.gifWidth}
              onChange={(e) => set('gifWidth', Number(e.target.value))}
              className={selectCls}
            >
              {[480, 640, 800, 960].map((v) => (
                <option key={v} value={v}>
                  {v} px
                </option>
              ))}
            </select>
          </Field>
          <Field label="Scroll delay (ms)">
            <input
              type="number"
              min={100}
              max={1500}
              step={50}
              value={draft.scrollDelayMs}
              onChange={(e) => set('scrollDelayMs', Number(e.target.value))}
              className={selectCls}
            />
          </Field>
          <Field label="Scroll wheel ticks per step">
            <input
              type="number"
              min={1}
              max={15}
              value={draft.scrollStepTicks}
              onChange={(e) => set('scrollStepTicks', Number(e.target.value))}
              className={selectCls}
            />
          </Field>
          <Field label="Max page height (scroll capture)">
            <select
              value={draft.maxScrollHeight}
              onChange={(e) => set('maxScrollHeight', Number(e.target.value))}
              className={selectCls}
            >
              {[10000, 15000, 20000, 30000, 50000].map((v) => (
                <option key={v} value={v}>
                  {v} px
                </option>
              ))}
            </select>
          </Field>
          <Field label="Auto-delete library items">
            <select
              value={draft.autoDeleteDays}
              onChange={(e) => set('autoDeleteDays', Number(e.target.value))}
              className={selectCls}
            >
              {[
                [0, 'Never'],
                [1, 'After 1 day'],
                [7, 'After 7 days'],
                [30, 'After 30 days'],
                [90, 'After 90 days'],
                [365, 'After 365 days'],
              ].map(([v, label]) => (
                <option key={v} value={v}>
                  {label}
                </option>
              ))}
            </select>
          </Field>
          <div className="border-t border-zinc-800 pt-4">
            <span className="mb-2 block text-xs font-medium text-zinc-400">Keyboard shortcuts</span>
            <div className="space-y-2">
              {hotkeyFields.map(([key, label, hint]) => (
                <HotkeyInput
                  key={key}
                  label={label}
                  hint={hint}
                  value={draft[key] as string}
                  usedBy={hotkeyUseCounts.get((draft[key] as string).trim())?.filter((l) => l !== label) ?? []}
                  onChange={(v) => set(key, v)}
                />
              ))}
            </div>
            <p className="mt-2 text-xs text-zinc-500">
              Works system-wide while the app runs. Click a field and press the keys; Backspace
              clears one, Esc cancels. An empty shortcut disables it. Shortcuts are inactive while
              this dialog is open, and if macOS or another app owns a combination, the default is
              kept instead.
            </p>
          </div>
          <div className="border-t border-zinc-800 pt-4">
            <span className="mb-2 block text-xs font-medium text-zinc-400">Troubleshooting</span>
            <div className="flex flex-wrap gap-2">
              <button
                onClick={copyDiagnostics}
                className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-300 transition hover:border-emerald-500/50 hover:text-emerald-400"
              >
                Copy diagnostics
              </button>
              <button
                onClick={openLogFolder}
                className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-300 transition hover:border-emerald-500/50 hover:text-emerald-400"
              >
                Open log folder
              </button>
            </div>
            <p className="mt-2 text-xs text-zinc-500">
              Diagnostics list your displays and capture details with the last log lines. Paste them
              when reporting a capture problem.
            </p>
            {diagnostics === 'copied' && (
              <p role="status" className="mt-1 text-xs text-emerald-400">
                Copied to the clipboard.
              </p>
            )}
            {diagnostics === 'failed' && (
              <p role="status" className="mt-1 text-xs text-red-400">
                That did not work. The log folder is listed under “Where are the logs?” in the README.
              </p>
            )}
          </div>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-zinc-800 px-6 py-4">
          <button
            onClick={onClose}
            className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-300 transition hover:border-zinc-500"
          >
            Cancel
          </button>
          <button
            onClick={save}
            className="rounded-lg bg-emerald-500 px-3 py-2 text-sm font-medium text-zinc-950 transition hover:bg-emerald-400"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

const isMac = /Mac/i.test(navigator.platform);

/** The primary modifier, stored and shown differently per platform. */
const PRIMARY = isMac ? 'Cmd' : 'Ctrl';

/** KeyboardEvent.code -> the shortcut plugin's key token. */
const KEY_TOKENS: Record<string, string> = {
  Space: 'Space',
  PrintScreen: 'PrintScreen',
  ScrollLock: 'ScrollLock',
  Pause: 'Pause',
  Insert: 'Insert',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Minus: 'Minus',
  Equal: 'Equal',
  BracketLeft: 'BracketLeft',
  BracketRight: 'BracketRight',
  Backquote: 'Backquote',
  Backslash: 'Backslash',
  Comma: 'Comma',
  Period: 'Period',
  Slash: 'Slash',
  Semicolon: 'Semicolon',
  Quote: 'Quote',
  Tab: 'Tab',
};

function keyToken(code: string): string | null {
  if (KEY_TOKENS[code]) return KEY_TOKENS[code];
  let match = code.match(/^Key([A-Z])$/);
  if (match) return match[1];
  match = code.match(/^Digit(\d)$/);
  if (match) return match[1];
  match = code.match(/^Numpad(\d)$/);
  if (match) return `Num${match[1]}`;
  if (/^F([1-9]|1\d|2[0-4])$/.test(code)) return code;
  return null;
}

/** "Cmd+Shift+6" as the user sees it. */
function pretty(value: string): string {
  return value
    .split('+')
    .filter(Boolean)
    .map((part) => (isMac ? { Cmd: '⌘', Ctrl: '⌃', Alt: '⌥', Shift: '⇧' }[part] : { Super: 'Win' }[part]) ?? part)
    .join(isMac ? ' ' : '+');
}

function HotkeyInput(props: {
  label: string;
  hint: string;
  value: string;
  /** Labels of other actions sharing this shortcut in the current draft. */
  usedBy: string[];
  onChange: (value: string) => void;
}) {
  const { label, hint, value, usedBy, onChange } = props;
  const [recording, setRecording] = useState(false);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      setRecording(false);
      return;
    }
    if (e.key === 'Backspace') {
      e.preventDefault();
      onChange('');
      setRecording(false);
      return;
    }
    if (['Control', 'Meta', 'Alt', 'Shift', 'AltGraph', 'Fn', 'FnLock'].includes(e.key)) return;
    const token = keyToken(e.code) ?? (e.key.length === 1 ? e.key.toUpperCase() : null);
    if (!token) return;
    e.preventDefault();
    const mods: string[] = [];
    if (e.metaKey) mods.push(isMac ? 'Cmd' : 'Super');
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');
    // A bare letter or digit as a system-wide shortcut fires while typing.
    if (mods.length === 0 && /^[A-Z0-9]$/.test(token)) return;
    onChange([...mods, token].join('+'));
    setRecording(false);
  };

  const duplicate = usedBy.length > 0;

  return (
    <div>
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-zinc-300" title={hint}>
          {label}
        </span>
        {value && !recording && (
          <button
            type="button"
            title="Disable this shortcut"
            aria-label={`Disable the ${label} shortcut`}
            onClick={() => onChange('')}
            className="text-xs text-zinc-500 transition hover:text-zinc-300"
          >
            Clear
          </button>
        )}
      </div>
      <div
        role="button"
        tabIndex={0}
        aria-label={`${label} shortcut`}
        onClick={() => setRecording(true)}
        onKeyDown={onKeyDown}
        onBlur={() => setRecording(false)}
        className={`w-full cursor-pointer select-none rounded-md border px-2.5 py-2 text-sm focus:outline-none ${
          recording
            ? 'border-emerald-500/60 text-emerald-400'
            : duplicate
              ? 'border-red-500/50 text-red-300'
              : 'border-zinc-800 bg-zinc-950 text-zinc-200'
        }`}
      >
        {recording ? 'Press keys… (Esc cancels, Backspace clears)' : value ? pretty(value) : 'Not set'}
      </div>
      {duplicate && (
        <p className="mt-1 text-xs text-red-400">Also set for {usedBy.join(', ')}</p>
      )}
    </div>
  );
}
