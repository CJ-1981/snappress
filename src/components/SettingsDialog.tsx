import { X } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { api } from '../lib/api';
import type { AppSettings } from '../lib/types';

interface Props {
  open: boolean;
  settings: AppSettings;
  onClose: () => void;
  onSaved: (s: AppSettings) => void;
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

  const set = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));

  const save = async () => {
    try {
      const s = await api.setSettings(draft);
      onSaved(s);
      onClose();
    } catch {
      /* dialog stays open on failure */
    }
  };

  return (
    <div
      className="fixed inset-0 z-[70] flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      onClick={onClose}
    >
      <div
        className="w-full max-w-md rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold">Settings</h2>
          <button
            onClick={onClose}
            aria-label="Close settings"
            className="rounded-md p-1.5 text-zinc-400 transition hover:bg-zinc-800 hover:text-zinc-200"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

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
        </div>

        <div className="mt-6 flex justify-end gap-2">
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
